/* esp32-remote-access
 * Copyright (C) 2024 Frederic Henrichs <frederic@tinkerforge.com>
 *
 * This library is free software; you can redistribute it and/or
 * modify it under the terms of the GNU Lesser General Public
 * License as published by the Free Software Foundation; either
 * version 2 of the License, or (at your option) any later version.
 *
 * This library is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU
 * Lesser General Public License for more details.
 *
 * You should have received a copy of the GNU Lesser General Public
 * License along with this library; if not, write to the
 * Free Software Foundation, Inc., 59 Temple Place - Suite 330,
 * Boston, MA 02111-1307, USA.
 */

use std::{
    collections::HashSet,
    time::{Instant, SystemTime, UNIX_EPOCH},
};

use actix_web::{put, web, HttpRequest, HttpResponse, Responder};
use db_connector::models::{allowed_users::AllowedUser, users::User};
use diesel::{prelude::*, result::Error::NotFound};
use ipnetwork::IpNetwork;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::{
    error::Error,
    rate_limit::ChargerRateLimiter,
    routes::{auth::login::FindBy, user::get_user_id},
    utils::{
        get_charger_by_uid, get_charger_from_db, get_connection, parse_uuid,
        update_charger_state_change, web_block_unpacked,
    },
    AppState, BridgeState,
};

use super::charger::add::password_matches;

#[derive(Serialize, Deserialize, ToSchema)]
pub struct ManagementSchema {
    // In a future version we will break the api and Only use ManagementDataVersion as expected body
    pub id: Option<i32>,
    pub password: Option<String>,
    pub data: ManagementDataVersion,
}

#[derive(Serialize, Deserialize, ToSchema)]
pub enum ManagementDataVersion {
    V1(ManagementDataVersion1),
    V2(ManagementDataVersion2),
}

#[derive(Serialize, Deserialize, ToSchema, Debug)]
pub struct ConfiguredUser {
    pub email: Option<String>,
    pub user_id: Option<String>,
    // Encrypted charger name
    pub name: Option<String>,
}

#[derive(Serialize, Deserialize, ToSchema)]
pub struct ManagementDataVersion2 {
    pub id: String,
    pub password: String,
    pub port: u16,
    pub firmware_version: String,
    pub configured_users: Vec<ConfiguredUser>,
    pub mtu: Option<u16>,
}

#[derive(Serialize, Deserialize, ToSchema)]
pub struct ManagementDataVersion1 {
    pub port: u16,
    pub firmware_version: String,
    pub configured_connections: Vec<i32>,
}

#[derive(Serialize, Deserialize, ToSchema, Debug)]
pub struct ManagementResponseSchema {
    pub time: u64,
    pub configured_users: Vec<i32>,
    pub configured_users_emails: Vec<String>,
    pub configured_users_uuids: Vec<String>,
    pub uuid: Option<String>,
}

async fn identify_configured_user(
    user: &ConfiguredUser,
    state: &web::Data<AppState>,
) -> Option<uuid::Uuid> {
    let user_id = if let Some(email) = &user.email {
        match get_user_id(state, FindBy::Email(email.to_string().to_lowercase())).await {
            Ok(u) => u,
            Err(_err) => return None,
        }
    } else if let Some(user_id) = &user.user_id {
        let user_id = match parse_uuid(user_id) {
            Ok(u) => u,
            Err(_err) => return None,
        };
        match get_user_id(state, FindBy::Uuid(user_id)).await {
            Ok(u) => u,
            Err(_err) => return None,
        }
    } else {
        return None;
    };

    Some(user_id)
}

async fn update_configured_users(
    state: &web::Data<AppState>,
    charger_id: uuid::Uuid,
    data: &ManagementDataVersion,
) -> actix_web::Result<(Vec<i32>, Vec<String>, Vec<String>)> {
    let configured_users = if let ManagementDataVersion::V2(data) = data {
        // Get uuids of configured users on wallbox
        let mut configured_users: Vec<uuid::Uuid> = Vec::new();
        for user in data.configured_users.iter() {
            use db_connector::schema::allowed_users::dsl as allowed_users;

            let user_id = match identify_configured_user(user, state).await {
                Some(u) => u,
                // Users could probe if a user is configured on the charger
                // So we add a random uuid to the list
                None => {
                    configured_users.push(uuid::Uuid::new_v4());
                    continue;
                }
            };

            if let Some(name) = &user.name {
                // Update name of charger for each user
                let mut conn = get_connection(state)?;
                match diesel::update(
                    allowed_users::allowed_users
                        .filter(allowed_users::user_id.eq(user_id))
                        .filter(allowed_users::charger_id.eq(charger_id)),
                )
                .set(allowed_users::name.eq(name))
                .execute(&mut conn)
                {
                    Ok(_) => (),
                    Err(NotFound) => (),
                    Err(_) => return Err(Error::InternalError.into()),
                }
            }

            configured_users.push(user_id);
        }

        // Delete allowed users not configured on the charger
        let configured_users_cpy = configured_users.clone();
        let mut conn = get_connection(state)?;
        let deleted_users = web_block_unpacked(move || {
            use db_connector::schema::allowed_users::dsl as allowed_users;

            let users_to_delete: Vec<uuid::Uuid> = match allowed_users::allowed_users
                .filter(allowed_users::charger_id.eq(&charger_id))
                .filter(allowed_users::user_id.ne_all(&configured_users_cpy))
                .select(AllowedUser::as_select())
                .load(&mut conn)
            {
                Ok(v) => v.into_iter().map(|u: AllowedUser| u.user_id).collect(),
                Err(NotFound) => return Ok(Vec::new()),
                Err(_err) => return Err(Error::InternalError),
            };

            match diesel::delete(
                allowed_users::allowed_users
                    .filter(allowed_users::charger_id.eq(&charger_id))
                    .filter(allowed_users::user_id.ne_all(configured_users_cpy)),
            )
            .execute(&mut conn)
            {
                Ok(_) => Ok(users_to_delete),
                Err(NotFound) => Ok(users_to_delete),
                Err(_err) => Err(Error::InternalError),
            }
        })
        .await?;

        if !deleted_users.is_empty() {
            let mut conn = get_connection(state)?;
            web_block_unpacked(move || {
                use db_connector::schema::wg_keys::dsl as wg_keys;

                match diesel::delete(
                    wg_keys::wg_keys
                        .filter(wg_keys::charger_id.eq(&charger_id))
                        .filter(wg_keys::user_id.eq_any(deleted_users)),
                )
                .execute(&mut conn)
                {
                    Ok(_) => Ok(()),
                    Err(_err) => Err(Error::InternalError),
                }
            })
            .await?;
        }

        // Get uuid of configured users on the server
        let mut conn = get_connection(state)?;
        let server_users: Vec<uuid::Uuid> = web_block_unpacked(move || {
            use db_connector::schema::allowed_users::dsl as allowed_users;

            match allowed_users::allowed_users
                .filter(allowed_users::charger_id.eq(&charger_id))
                .select(AllowedUser::as_select())
                .load(&mut conn)
            {
                Ok(u) => Ok(u.into_iter().map(|u: AllowedUser| u.user_id).collect()),
                Err(NotFound) => Ok(Vec::new()),
                Err(_err) => Err(Error::InternalError),
            }
        })
        .await?;

        // Resolve the E-Mail for each user
        let mut conn = get_connection(state)?;
        let server_users: Vec<User> = web_block_unpacked(move || {
            use db_connector::schema::users::dsl::*;

            match users
                .filter(id.eq_any(&server_users))
                .select(User::as_select())
                .load(&mut conn)
            {
                Ok(u) => Ok(u),
                Err(NotFound) => Ok(Vec::new()),
                Err(_err) => Err(Error::InternalError),
            }
        })
        .await?;

        // Used by the old api
        // TODO: Deprecate this
        let mut common_users_old = Vec::new();
        // Used by the new api
        let mut common_users_emails = Vec::new();
        let mut common_users_uuids = Vec::new();
        for u in configured_users.iter() {
            if let Some(idx) = server_users.iter().position(|su| su.id == *u) {
                common_users_emails.push(server_users[idx].email.clone());
                common_users_uuids.push(server_users[idx].id.to_string());
                common_users_old.push(1);
            } else {
                common_users_emails.push(String::new());
                common_users_uuids.push(String::new());
                common_users_old.push(0)
            }
        }

        (common_users_old, common_users_emails, common_users_uuids)
    } else {
        (Vec::new(), Vec::new(), Vec::new())
    };

    Ok(configured_users)
}

/// Route for the charger to be identifiable via the ip.
#[utoipa::path(
    request_body = ManagementSchema,
    responses(
        (status = 200, description = "Identification was successful", body = ManagementResponseSchema),
        (status = 400, description = "Got no valid ip address for the charger"),
        (status = 401, description = "The charger id and password do not match")
    )
)]
#[put("/management")]
pub async fn management(
    req: HttpRequest,
    state: web::Data<AppState>,
    data: web::Json<ManagementSchema>,
    bridge_state: web::Data<BridgeState>,
    rate_limiter: web::Data<ChargerRateLimiter>,
) -> actix_web::Result<impl Responder> {
    use db_connector::schema::chargers::dsl as chargers;

    let ip = {
        let info = req.connection_info();
        let ip = info.realip_remote_addr();

        if ip.is_none() {
            return Err(Error::NoValidIp.into());
        }

        ip.unwrap().to_owned()
    };

    let charger_id;
    let mut output_uuid = None;
    let charger = if let Some(charger_uid) = data.id {
        rate_limiter.check(charger_uid.to_string(), &req)?;

        let charger = get_charger_by_uid(charger_uid, data.password.clone(), &state).await?;
        charger_id = charger.id;
        output_uuid = Some(charger_id.to_string());
        charger
    } else {
        match &data.data {
            ManagementDataVersion::V1(_) => return Err(Error::ChargerCredentialsWrong.into()),
            ManagementDataVersion::V2(data) => {
                rate_limiter.check(data.id.clone(), &req)?;

                charger_id = parse_uuid(&data.id)?;
                let charger = get_charger_from_db(charger_id, &state).await?;
                if !password_matches(&data.password, &charger.password)? {
                    return Err(Error::ChargerCredentialsWrong.into());
                }
                charger
            }
        }
    };

    let ip: IpNetwork = match ip.parse() {
        Ok(ip) => ip,
        Err(_err) => {
            log::error!("Error while parsing ip: {_err}");
            return Err(Error::InternalError.into());
        }
    };

    let configured_users = update_configured_users(&state, charger_id, &data.data).await?;

    {
        let mut map = bridge_state.undiscovered_chargers.lock().await;
        let set = map.entry(ip).or_insert(HashSet::new());
        set.insert(crate::DiscoveryCharger {
            id: charger.id,
            last_request: Instant::now(),
        });
    }

    {
        let mut map = bridge_state.charger_management_map_with_id.lock().await;
        let sock = map.remove(&charger_id);
        if let Some(socket) = sock {
            let mut map = bridge_state.charger_management_map.lock().await;
            let socket = socket.lock().await;
            let _ = map.remove(&socket.get_remote_address());
        }
    }

    let addresses = {
        let mut map = bridge_state.charger_remote_conn_map.lock().await;
        let mut addresses = Vec::new();
        map.retain(|key, addr| {
            if key.charger_id == charger_id {
                addresses.push((*addr, key.conn_no));
                false
            } else {
                true
            }
        });
        addresses
    };

    let losing_conns = {
        let mut clients = bridge_state.web_client_map.lock().await;
        let mut losing_conns = Vec::new();
        for (addr, conn_no) in addresses.into_iter() {
            if let Some(recipient) = clients.remove(&addr) {
                losing_conns.push((conn_no, recipient));
            }
        }
        losing_conns
    };

    {
        let mut lost_conns = bridge_state.lost_connections.lock().await;
        lost_conns.insert(charger_id, losing_conns);
    }

    let (fw_version, port, mtu) = match &data.data {
        ManagementDataVersion::V1(v) => (v.firmware_version.clone(), v.port, None),
        ManagementDataVersion::V2(v) => (v.firmware_version.clone(), v.port, v.mtu),
    };

    let user_agent = req.headers().get("User-Agent");
    let device_type = user_agent.and_then(|h| h.to_str().ok()).and_then(|ua| {
        if ua == "ESP32 HTTP Client/1.0" {
            None
        } else {
            Some(ua.to_string())
        }
    });

    let mut conn = get_connection(&state)?;
    web_block_unpacked(move || {
        match diesel::update(chargers::chargers)
            .filter(chargers::id.eq(charger_id))
            .set((
                chargers::firmware_version.eq(fw_version),
                chargers::webinterface_port.eq(port as i32),
                chargers::device_type.eq(device_type),
                chargers::mtu.eq(mtu.map(|m| m as i32)),
            ))
            .execute(&mut conn)
        {
            Ok(_) => Ok(()),
            Err(_err) => {
                log::error!("Error while updating charger: {_err}");
                Err(Error::InternalError)
            }
        }
    })
    .await?;

    update_charger_state_change(charger_id, state).await;

    let time = match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(time) => time,
        Err(err) => {
            log::error!("Error while getting current time: {err}");
            return Err(Error::InternalError.into());
        }
    };

    let time = time.as_secs();
    let resp = ManagementResponseSchema {
        time,
        configured_users: configured_users.0,
        configured_users_emails: configured_users.1,
        configured_users_uuids: configured_users.2,
        uuid: output_uuid,
    };

    Ok(HttpResponse::Ok().json(resp))
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use super::*;
    use actix_web::{cookie::Cookie, test, App};
    use base64::{prelude::BASE64_STANDARD, Engine};
    use db_connector::{
        models::{allowed_users::AllowedUser, wg_keys::WgKey},
        test_connection_pool,
    };
    use rand::distr::{Alphanumeric, SampleString};

    use crate::{
        routes::{
            charger::allow_user::UserAuth,
            user::tests::{get_test_uuid, TestUser},
        },
        tests::configure,
    };

    #[actix_web::test]
    async fn test_management() {
        let (mut user, mail) = TestUser::random().await;
        user.login().await;
        let charger = user.add_random_charger().await;

        let app = App::new().configure(configure).service(management);
        let app = test::init_service(app).await;

        let user_id = get_test_uuid(&mail).unwrap();
        let charger_uuid_clone = charger.uuid.clone();
        let data = ManagementDataVersion::V2(ManagementDataVersion2 {
            id: charger.uuid,
            password: charger.password,
            port: 0,
            firmware_version: "2.3.1".to_string(),
            configured_users: vec![ConfiguredUser {
                email: None,
                user_id: Some(user_id.to_string()),
                name: Some(String::new()),
            }],
            mtu: None,
        });

        let body = ManagementSchema {
            id: None,
            password: None,
            data,
        };
        let req = test::TestRequest::put()
            .uri("/management")
            .append_header(("X-Forwarded-For", "123.123.123.3"))
            .append_header(("User-Agent", "Tinkerforge-WARP2_Charger/2.8.0+6811d0b1"))
            .set_json(body)
            .to_request();
        let resp: ManagementResponseSchema = test::call_and_read_body_json(&app, req).await;

        println!("{resp:?}");
        assert_eq!([1], *resp.configured_users);
        assert_eq!(vec![user_id.to_string()], resp.configured_users_uuids);

        // Verify device_type stored correctly
        use db_connector::models::chargers::Charger as DbCharger;
        use db_connector::schema::chargers::dsl::*;
        let pool = db_connector::test_connection_pool();
        let mut conn = pool.get().unwrap();
        let db_charger: DbCharger = chargers
            .filter(id.eq(uuid::Uuid::from_str(&charger_uuid_clone).unwrap()))
            .select(DbCharger::as_select())
            .get_result(&mut conn)
            .unwrap();
        assert_eq!(
            db_charger.device_type.as_deref(),
            Some("Tinkerforge-WARP2_Charger/2.8.0+6811d0b1")
        );
    }

    #[actix_web::test]
    async fn test_management_old_api() {
        let (mut user, mail) = TestUser::random().await;
        user.login().await;
        let charger = user.add_random_charger().await;

        let app = App::new().configure(configure).service(management);
        let app = test::init_service(app).await;

        let data = ManagementDataVersion::V2(ManagementDataVersion2 {
            id: charger.uuid,
            password: charger.password,
            port: 0,
            firmware_version: "2.3.1".to_string(),
            configured_users: vec![ConfiguredUser {
                email: Some(mail.to_uppercase()),
                user_id: None,
                name: Some(String::new()),
            }],
            mtu: None,
        });

        let body = ManagementSchema {
            id: None,
            password: None,
            data,
        };
        let req = test::TestRequest::put()
            .uri("/management")
            .append_header(("X-Forwarded-For", "123.123.123.3"))
            .set_json(body)
            .to_request();
        let resp: ManagementResponseSchema = test::call_and_read_body_json(&app, req).await;

        println!("{resp:?}");
        assert_eq!([1], *resp.configured_users);
        let user_id = get_test_uuid(&mail).unwrap();
        assert_eq!(vec![user_id.to_string()], resp.configured_users_uuids);
    }

    #[actix_web::test]
    async fn test_management_v1_api() {
        let (mut user, _) = TestUser::random().await;
        user.login().await;
        let charger = user.add_random_charger().await;

        let app = App::new().configure(configure).service(management);
        let app = test::init_service(app).await;

        let data = ManagementDataVersion::V1(ManagementDataVersion1 {
            port: 0,
            firmware_version: "2.3.1".to_string(),
            configured_connections: vec![0, 1, 2, 3, 4],
        });

        let body = ManagementSchema {
            id: Some(charger.uid),
            password: Some(charger.password),
            data,
        };
        let req = test::TestRequest::put()
            .uri("/management")
            .append_header(("X-Forwarded-For", "123.123.123.3"))
            .set_json(body)
            .to_request();
        let resp: ManagementResponseSchema = test::call_and_read_body_json(&app, req).await;

        println!("{resp:?}");
        assert_eq!([0; 0], *resp.configured_users);
        assert!(resp.configured_users_uuids.is_empty());
    }

    #[actix_web::test]
    async fn test_two_chargers_with_with_same_uid() {
        let (mut user, _) = TestUser::random().await;
        user.login().await;
        let charger = user.add_random_charger().await;
        let (mut user2, _) = TestUser::random().await;
        user2.login().await;
        let _charger2 = user2.add_charger(charger.uid).await;

        let app = App::new().configure(configure).service(management);
        let app = test::init_service(app).await;

        let data = ManagementDataVersion::V1(ManagementDataVersion1 {
            port: 0,
            firmware_version: "2.3.1".to_string(),
            configured_connections: vec![0, 1, 2, 3, 4],
        });

        let body = ManagementSchema {
            id: Some(charger.uid),
            password: Some(charger.password),
            data,
        };
        let req = test::TestRequest::put()
            .uri("/management")
            .append_header(("X-Forwarded-For", "123.123.123.3"))
            .set_json(body)
            .to_request();
        let resp: ManagementResponseSchema = test::call_and_read_body_json(&app, req).await;

        println!("{resp:?}");
        assert_eq!([0; 0], *resp.configured_users);
        assert!(resp.configured_users_uuids.is_empty());
    }

    #[actix_web::test]
    async fn test_wrong_password() {
        let (mut user, mail) = TestUser::random().await;
        user.login().await;
        let charger = user.add_random_charger().await;

        let app = App::new().configure(configure).service(management);
        let app = test::init_service(app).await;

        let data = ManagementDataVersion::V2(ManagementDataVersion2 {
            id: charger.uuid,
            password: Alphanumeric.sample_string(&mut rand::rng(), 32),
            port: 0,
            firmware_version: "2.3.1".to_string(),
            configured_users: vec![ConfiguredUser {
                email: None,
                user_id: Some(get_test_uuid(&mail).unwrap().to_string()),
                name: Some(String::new()),
            }],
            mtu: None,
        });
        let body = ManagementSchema {
            id: None,
            password: None,
            data,
        };
        let req = test::TestRequest::put()
            .uri("/management")
            .append_header(("X-Forwarded-For", "123.123.123.3"))
            .set_json(body)
            .to_request();
        let resp = test::call_service(&app, req).await;

        println!("{resp:?}");
        println!("{:?}", resp.response().body());
        assert!(resp.status().is_client_error());
        assert_eq!(resp.status().as_u16(), 401);
    }

    #[actix_web::test]
    async fn test_wrong_password_v1_api() {
        let (mut user, _) = TestUser::random().await;
        user.login().await;
        let charger = user.add_random_charger().await;

        let app = App::new().configure(configure).service(management);
        let app = test::init_service(app).await;

        let data = ManagementDataVersion::V1(ManagementDataVersion1 {
            port: 0,
            firmware_version: "2.3.1".to_string(),
            configured_connections: vec![0, 1, 2, 3, 4],
        });
        let body: ManagementSchema = ManagementSchema {
            id: Some(charger.uid),
            password: Some(Alphanumeric.sample_string(&mut rand::rng(), 32)),
            data,
        };
        let req = test::TestRequest::put()
            .uri("/management")
            .append_header(("X-Forwarded-For", "123.123.123.3"))
            .set_json(body)
            .to_request();
        let resp = test::call_service(&app, req).await;

        println!("{resp:?}");
        println!("{:?}", resp.response().body());
        assert!(resp.status().is_client_error());
        assert_eq!(resp.status().as_u16(), 401);
    }

    #[actix::test]
    async fn test_charger_removed_user() {
        let (mut user, mail) = TestUser::random().await;
        user.login().await;
        let charger = user.add_random_charger().await;
        let (mut user2, mail2) = TestUser::random().await;
        user2.login().await;
        user.allow_user(
            &mail2,
            UserAuth::LoginKey(BASE64_STANDARD.encode(&user2.get_login_key().await)),
            &charger,
        )
        .await;

        let app = App::new().configure(configure).service(management);
        let app = test::init_service(app).await;

        let data = ManagementDataVersion::V2(ManagementDataVersion2 {
            id: charger.uuid.clone(),
            password: charger.password,
            port: 0,
            firmware_version: "2.3.1".to_string(),
            configured_users: vec![ConfiguredUser {
                email: None,
                user_id: Some(get_test_uuid(&mail).unwrap().to_string()),
                name: Some(String::new()),
            }],
            mtu: None,
        });

        let body = ManagementSchema {
            id: None,
            password: None,
            data,
        };
        let req = test::TestRequest::put()
            .uri("/management")
            .append_header(("X-Forwarded-For", "123.123.123.3"))
            .cookie(Cookie::new("X-Forwarded-For", "123.123.123.3"))
            .set_json(body)
            .to_request();
        let resp: ManagementResponseSchema = test::call_and_read_body_json(&app, req).await;

        println!("{resp:?}");
        assert_eq!([1], *resp.configured_users);
        assert_eq!(
            vec![get_test_uuid(&mail).unwrap().to_string()],
            resp.configured_users_uuids
        );

        let pool = test_connection_pool();
        let mut conn = pool.get().unwrap();

        let user_id: uuid::Uuid = {
            use db_connector::schema::allowed_users::dsl::*;

            let cid = uuid::Uuid::from_str(&charger.uuid).unwrap();
            let user: AllowedUser = allowed_users
                .filter(charger_id.eq(cid))
                .select(AllowedUser::as_select())
                .get_result(&mut conn)
                .unwrap();
            user.user_id
        };
        assert_eq!(get_test_uuid(&user.mail).unwrap(), user_id);
        let wg_keys: Vec<WgKey> = {
            use db_connector::schema::wg_keys::dsl as wg_keys;

            wg_keys::wg_keys
                .filter(wg_keys::user_id.eq(get_test_uuid(&user2.mail).unwrap()))
                .select(WgKey::as_select())
                .load(&mut conn)
                .unwrap()
        };
        assert_eq!(wg_keys.len(), 0);
    }

    #[actix::test]
    async fn test_server_removed_user() {
        let (mut user, mail) = TestUser::random().await;
        user.login().await;
        let charger = user.add_random_charger().await;
        let (mut user2, mail2) = TestUser::random().await;
        user2.login().await;
        user.allow_user(
            &mail2,
            UserAuth::LoginKey(BASE64_STANDARD.encode(&user2.get_login_key().await)),
            &charger,
        )
        .await;

        {
            use db_connector::schema::allowed_users::dsl::*;

            let pool = test_connection_pool();
            let mut conn = pool.get().unwrap();
            let uuid = get_test_uuid(&mail2).unwrap();
            diesel::delete(allowed_users.filter(user_id.eq(&uuid)))
                .execute(&mut conn)
                .unwrap();
        }

        let app = App::new().configure(configure).service(management);
        let app = test::init_service(app).await;

        let data = ManagementDataVersion::V2(ManagementDataVersion2 {
            id: charger.uuid.clone(),
            password: charger.password,
            port: 0,
            firmware_version: "2.3.1".to_string(),
            configured_users: vec![
                ConfiguredUser {
                    email: None,
                    user_id: Some(get_test_uuid(&mail).unwrap().to_string()),
                    name: Some(String::new()),
                },
                ConfiguredUser {
                    email: None,
                    user_id: Some(get_test_uuid(&mail2).unwrap().to_string()),
                    name: Some(String::new()),
                },
            ],
            mtu: None,
        });

        let body = ManagementSchema {
            id: None,
            password: None,
            data,
        };
        let req = test::TestRequest::put()
            .uri("/management")
            .append_header(("X-Forwarded-For", "123.123.123.3"))
            .cookie(Cookie::new("X-Forwarded-For", "123.123.123.3"))
            .set_json(body)
            .to_request();
        let resp: ManagementResponseSchema = test::call_and_read_body_json(&app, req).await;

        println!("{resp:?}");
        assert_eq!([1, 0], *resp.configured_users);
        assert_eq!(
            vec![get_test_uuid(&mail).unwrap().to_string(), String::new()],
            resp.configured_users_uuids
        );

        let pool = test_connection_pool();
        let mut conn = pool.get().unwrap();

        let user_id: uuid::Uuid = {
            use db_connector::schema::allowed_users::dsl::*;

            let cid = uuid::Uuid::from_str(&charger.uuid).unwrap();
            let user: AllowedUser = allowed_users
                .filter(charger_id.eq(cid))
                .select(AllowedUser::as_select())
                .get_result(&mut conn)
                .unwrap();
            user.user_id
        };
        assert_eq!(get_test_uuid(&user.mail).unwrap(), user_id);
    }

    #[actix::test]
    async fn test_server_deleted_user() {
        let (mut user, mail) = TestUser::random().await;
        user.login().await;
        let charger = user.add_random_charger().await;

        let mail2 = {
            let (mut user2, mail2) = TestUser::random().await;
            user2.login().await;
            user.allow_user(
                &mail2,
                UserAuth::LoginKey(BASE64_STANDARD.encode(&user2.get_login_key().await)),
                &charger,
            )
            .await;

            mail2
        };

        let app = App::new().configure(configure).service(management);
        let app = test::init_service(app).await;

        let data = ManagementDataVersion::V2(ManagementDataVersion2 {
            id: charger.uuid.clone(),
            password: charger.password,
            port: 0,
            firmware_version: "2.3.1".to_string(),
            configured_users: vec![
                ConfiguredUser {
                    email: None,
                    user_id: Some(get_test_uuid(&mail).unwrap().to_string()),
                    name: Some(String::new()),
                },
                ConfiguredUser {
                    email: Some(mail2),
                    user_id: None,
                    name: Some(String::new()),
                },
            ],
            mtu: None,
        });

        let body = ManagementSchema {
            id: None,
            password: None,
            data,
        };
        let req = test::TestRequest::put()
            .uri("/management")
            .append_header(("X-Forwarded-For", "123.123.123.3"))
            .cookie(Cookie::new("X-Forwarded-For", "123.123.123.3"))
            .set_json(body)
            .to_request();
        let resp: ManagementResponseSchema = test::call_and_read_body_json(&app, req).await;

        println!("{resp:?}");
        assert_eq!([1, 0], *resp.configured_users);
        assert_eq!(
            vec![get_test_uuid(&mail).unwrap().to_string(), String::new()],
            resp.configured_users_uuids
        );

        let pool = test_connection_pool();
        let mut conn = pool.get().unwrap();

        let user_id: uuid::Uuid = {
            use db_connector::schema::allowed_users::dsl::*;

            let cid = uuid::Uuid::from_str(&charger.uuid).unwrap();
            let user: AllowedUser = allowed_users
                .filter(charger_id.eq(cid))
                .select(AllowedUser::as_select())
                .get_result(&mut conn)
                .unwrap();
            user.user_id
        };
        assert_eq!(get_test_uuid(&user.mail).unwrap(), user_id);
    }

    #[actix::test]
    async fn removed_connections_v1_api() {
        let (mut user, _) = TestUser::random().await;
        user.login().await;
        let charger = user.add_random_charger().await;

        let app = App::new().configure(configure).service(management);
        let app = test::init_service(app).await;

        let data = ManagementDataVersion::V1(ManagementDataVersion1 {
            port: 0,
            firmware_version: "2.3.1".to_string(),
            configured_connections: vec![0, 1, 2, 3],
        });

        let body = ManagementSchema {
            id: Some(charger.uid),
            password: Some(charger.password),
            data,
        };
        let req = test::TestRequest::put()
            .uri("/management")
            .append_header(("X-Forwarded-For", "123.123.123.3"))
            .cookie(Cookie::new("X-Forwarded-For", "123.123.123.3"))
            .set_json(body)
            .to_request();
        let resp: ManagementResponseSchema = test::call_and_read_body_json(&app, req).await;

        println!("{resp:?}");
        assert_eq!([0; 0], *resp.configured_users);
        assert!(resp.configured_users_uuids.is_empty());

        let pool = test_connection_pool();
        let mut conn = pool.get().unwrap();

        let user_id: uuid::Uuid = {
            use db_connector::schema::allowed_users::dsl::*;

            let cid = uuid::Uuid::from_str(&charger.uuid).unwrap();
            let user: AllowedUser = allowed_users
                .filter(charger_id.eq(cid))
                .select(AllowedUser::as_select())
                .get_result(&mut conn)
                .unwrap();
            user.user_id
        };
        assert_eq!(get_test_uuid(&user.mail).unwrap(), user_id);
    }

    #[actix_web::test]
    async fn test_management_invalid_users() {
        let (mut owner, _) = TestUser::random().await;
        owner.login().await;
        let charger = owner.add_random_charger().await;

        let (mut user2, mail2) = TestUser::random().await;
        user2.login().await;
        owner
            .allow_user(
                &mail2,
                UserAuth::LoginKey(BASE64_STANDARD.encode(&user2.get_login_key().await)),
                &charger,
            )
            .await;

        let app = App::new().configure(configure).service(management);
        let app = test::init_service(app).await;

        let body = ManagementSchema {
            id: None,
            password: None,
            data: ManagementDataVersion::V2(ManagementDataVersion2 {
                id: charger.uuid.clone(),
                password: charger.password,
                port: 4321,
                firmware_version: String::new(),
                configured_users: vec![
                    ConfiguredUser {
                        email: None,
                        name: Some(String::new()),
                        user_id: None,
                    },
                    ConfiguredUser {
                        email: None,
                        name: Some("Renamed".to_string()),
                        user_id: None,
                    },
                ],
                mtu: None,
            }),
        };

        let req = test::TestRequest::put()
            .uri("/management")
            .append_header(("X-Forwarded-For", "123.123.123.3"))
            .set_json(body)
            .to_request();

        let resp: ManagementResponseSchema = test::call_and_read_body_json(&app, req).await;
        // We expect no configured users recognized
        assert_eq!(resp.configured_users, [0, 0]);
        assert_eq!(resp.configured_users_emails, [String::new(), String::new()]);
        assert_eq!(
            vec![String::new(), String::new()],
            resp.configured_users_uuids
        );
    }
}
