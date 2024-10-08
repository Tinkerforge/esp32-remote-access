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
use db_connector::models::wg_keys::WgKey;
use diesel::prelude::*;
use ipnetwork::IpNetwork;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::{
    error::Error, routes::charger::add::get_charger_from_db, utils::{get_charger_by_uid, get_connection, parse_uuid, web_block_unpacked}, AppState, BridgeState
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

#[derive(Serialize, Deserialize, ToSchema)]
pub struct ManagementDataVersion2 {
    pub id: String,
    pub password: String,
    pub port: u16,
    pub firmware_version: String,
    pub configured_connections: Vec<i32>,
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
    pub configured_connections: Vec<i32>,
}

async fn update_configured_connections(
    state: &web::Data<AppState>,
    charger_id: uuid::Uuid,
    configured_connections: Vec<i32>,
) -> actix_web::Result<Vec<i32>> {
    let mut conn = get_connection(state)?;
    web_block_unpacked(move || {
        use db_connector::schema::wg_keys::dsl as wg_keys;

        match diesel::delete(
            wg_keys::wg_keys
                .filter(wg_keys::charger_id.eq(charger_id))
                .filter(wg_keys::connection_no.ne_all(configured_connections)),
        )
        .execute(&mut conn)
        {
            Ok(_) => Ok(()),
            Err(_err) => Err(Error::InternalError),
        }
    })
    .await?;

    let mut conn = get_connection(state)?;
    let configured_connections: Vec<WgKey> = web_block_unpacked(move || {
        use db_connector::schema::wg_keys::dsl as wg_keys;

        match wg_keys::wg_keys
            .filter(wg_keys::charger_id.eq(charger_id))
            .select(WgKey::as_select())
            .load(&mut conn)
        {
            Ok(v) => Ok(v),
            Err(_err) => Err(Error::InternalError),
        }
    })
    .await?;

    // Remove users that are not present on the wallbox anymore
    let uids: Vec<uuid::Uuid> = configured_connections.iter().map(|c| c.user_id).collect();
    let mut conn = get_connection(state)?;
    web_block_unpacked(move || {
        use db_connector::schema::allowed_users::dsl as allowed_users;

        match diesel::delete(
            allowed_users::allowed_users
                .filter(allowed_users::charger_id.eq(charger_id))
                .filter(allowed_users::user_id.ne_all(uids)),
        )
        .execute(&mut conn)
        {
            Ok(_) => Ok(()),
            Err(_err) => Err(Error::InternalError),
        }
    })
    .await?;

    let configured_connections: Vec<i32> = configured_connections
        .iter()
        .map(|c| c.connection_no)
        .collect();

    Ok(configured_connections)
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
) -> actix_web::Result<impl Responder> {
    use db_connector::schema::chargers::dsl as chargers;

    let info = req.connection_info();
    let ip = info.realip_remote_addr();

    if ip.is_none() {
        return Err(Error::NoValidIp.into());
    }

    let ip = ip.unwrap();

    let charger_id;
    let charger = if let Some(charger_uid) = data.id {
        let charger = get_charger_by_uid(charger_uid, data.password.clone(), &state).await?;
        charger_id = charger.id;
        charger
    } else {
        match &data.data {
            ManagementDataVersion::V1(_) => return Err(Error::ChargerCredentialsWrong.into()),
            ManagementDataVersion::V2(data) => {
                charger_id = parse_uuid(&data.id)?;
                let charger = get_charger_from_db(charger_id, &state).await?;
                if !password_matches(&data.password, &charger.password)? {
                    return Err(Error::ChargerCredentialsWrong.into())
                }
                charger
            }
        }
    };

    let ip: IpNetwork = match ip.parse() {
        Ok(ip) => ip,
        Err(_err) => {
            log::error!("Error while parsing ip: {}", _err);
            return Err(Error::InternalError.into());
        }
    };

    let configured_connections = match &data.data {
        ManagementDataVersion::V1(data) => data.configured_connections.clone(),
        ManagementDataVersion::V2(data) => data.configured_connections.clone(),
    };
    let configured_connections =
        update_configured_connections(&state, charger.id, configured_connections).await?;

    {
        let mut map = bridge_state.undiscovered_chargers.lock().unwrap();
        let set = map.entry(ip).or_insert(HashSet::new());
        set.insert(crate::DiscoveryCharger {
            id: charger.id,
            last_request: Instant::now(),
        });
    }

    {
        let mut map = bridge_state.charger_management_map_with_id.lock().unwrap();
        let sock = map.remove(&data.id);
        if let Some(socket) = sock {
            let mut map = bridge_state.charger_management_map.lock().unwrap();
            let socket = socket.lock().unwrap();
            let _ = map.remove(&socket.get_remote_address());
        }
    }

    let addresses = {
        let mut map = bridge_state.charger_remote_conn_map.lock().unwrap();
        let mut addresses = Vec::new();
        map.retain(|key, addr| {
            if key.charger_id == data.id {
                addresses.push((*addr, key.conn_no));
                false
            } else {
                true
            }
        });
        addresses
    };

    let losing_conns = {
        let mut clients = bridge_state.web_client_map.lock().unwrap();
        let mut losing_conns = Vec::new();
        for (addr, conn_no) in addresses.into_iter() {
            if let Some(recipient) = clients.remove(&addr) {
                losing_conns.push((conn_no, recipient));
            }
        }
        losing_conns
    };

    {
        let mut lost_conns = bridge_state.lost_connections.lock().unwrap();
        lost_conns.insert(data.id, losing_conns);
    }

    let (fw_version, port) = match &data.data {
        ManagementDataVersion::V1(v) => (v.firmware_version.clone(), v.port),
        ManagementDataVersion::V2(v) => (v.firmware_version.clone(), v.port),
    };
    let mut conn = get_connection(&state)?;
    web_block_unpacked(move || {
        match diesel::update(chargers::chargers)
            .filter(chargers::id.eq(charger_id))
            .set((
                chargers::firmware_version.eq(fw_version),
                chargers::webinterface_port.eq(port as i32),
            ))
            .execute(&mut conn)
        {
            Ok(_) => Ok(()),
            Err(_err) => {
                log::error!("Error while updating charger: {}", _err);
                return Err(Error::InternalError.into());
            }
        }
    })
    .await?;

    let time = match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(time) => time,
        Err(err) => {
            log::error!("Error while getting current time: {}", err);
            return Err(Error::InternalError.into());
        }
    };

    let time = time.as_secs();
    let resp = ManagementResponseSchema {
        time,
        configured_connections,
    };

    Ok(HttpResponse::Ok().json(resp))
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use super::*;
    use actix_web::{cookie::Cookie, test, App};
    use db_connector::{models::allowed_users::AllowedUser, test_connection_pool};
    use rand::distributions::{Alphanumeric, DistString};

    use crate::{
        routes::user::tests::{get_test_uuid, TestUser},
        tests::configure,
    };

    #[actix_web::test]
    async fn test_management() {
        let (mut user, _) = TestUser::random().await;
        user.login().await;
        let charger = user.add_random_charger().await;

        let app = App::new().configure(configure).service(management);
        let app = test::init_service(app).await;

        let data = ManagementDataVersion::V2(ManagementDataVersion2 {
            id: charger.uuid,
            password: charger.password,
            port: 0,
            firmware_version: "2.3.1".to_string(),
            configured_connections: vec![0, 1, 2, 3, 4],
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

        println!("{:?}", resp);
        assert_eq!([0, 1, 2, 3, 4], *resp.configured_connections);
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
            .cookie(Cookie::new("X-Forwarded-For", "123.123.123.3"))
            .set_json(body)
            .to_request();
        let resp: ManagementResponseSchema = test::call_and_read_body_json(&app, req).await;

        println!("{:?}", resp);
        assert_eq!([0, 1, 2, 3, 4], *resp.configured_connections);
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
            .cookie(Cookie::new("X-Forwarded-For", "123.123.123.3"))
            .set_json(body)
            .to_request();
        let resp: ManagementResponseSchema = test::call_and_read_body_json(&app, req).await;

        println!("{:?}", resp);
        assert_eq!([0, 1, 2, 3, 4], *resp.configured_connections);
    }

    #[actix_web::test]
    async fn test_wrong_password() {
        let (mut user, _) = TestUser::random().await;
        user.login().await;
        let charger = user.add_random_charger().await;

        let app = App::new().configure(configure).service(management);
        let app = test::init_service(app).await;

        let data = ManagementDataVersion::V2(ManagementDataVersion2 {
            id: charger.uuid,
            password: Alphanumeric.sample_string(&mut rand::thread_rng(), 32),
            port: 0,
            firmware_version: "2.3.1".to_string(),
            configured_connections: vec![0, 1, 2, 3, 4],
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

        println!("{:?}", resp);
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
            password: Some(Alphanumeric.sample_string(&mut rand::thread_rng(), 32)),
            data,
        };
        let req = test::TestRequest::put()
            .uri("/management")
            .append_header(("X-Forwarded-For", "123.123.123.3"))
            .set_json(body)
            .to_request();
        let resp = test::call_service(&app, req).await;

        println!("{:?}", resp);
        println!("{:?}", resp.response().body());
        assert!(resp.status().is_client_error());
        assert_eq!(resp.status().as_u16(), 401);
    }

    #[actix::test]
    async fn removed_connections() {
        let (mut user, _) = TestUser::random().await;
        user.login().await;
        let charger = user.add_random_charger().await;

        let app = App::new().configure(configure).service(management);
        let app = test::init_service(app).await;

        let data = ManagementDataVersion::V2(ManagementDataVersion2 {
            id: charger.uuid.clone(),
            password: charger.password,
            port: 0,
            firmware_version: "2.3.1".to_string(),
            configured_connections: vec![0, 1, 2, 3],
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

        println!("{:?}", resp);
        assert_eq!([0, 1, 2, 3], *resp.configured_connections);

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
        assert_eq!(get_test_uuid(&user.mail), user_id);
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

        println!("{:?}", resp);
        assert_eq!([0, 1, 2, 3], *resp.configured_connections);

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
}
