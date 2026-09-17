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

use actix_web::{delete, web, HttpResponse, Responder};
use diesel::{ExpressionMethods, QueryDsl};
use diesel_async::RunQueryDsl as _;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::{
    error::Error,
    routes::charger::user_is_allowed,
    udp_server::management::prompt_charger_to_remove_user,
    utils::{get_connection, parse_uuid},
    AppState, BridgeState,
};

#[derive(Debug, Deserialize, Serialize, ToSchema)]
pub struct DeleteChargerSchema {
    charger: String,
}

pub async fn delete_all_keys(
    cid: uuid::Uuid,
    state: &web::Data<AppState>,
) -> Result<(), actix_web::Error> {
    use db_connector::schema::wg_keys::dsl::*;

    let mut conn = get_connection(state).await?;
    diesel::delete(wg_keys.filter(charger_id.eq(cid)))
        .execute(&mut conn)
        .await
        .map_err(|_| Error::InternalError)?;

    Ok(())
}

pub async fn delete_all_allowed_users(
    cid: uuid::Uuid,
    state: &web::Data<AppState>,
) -> Result<(), actix_web::Error> {
    use db_connector::schema::allowed_users::dsl::*;

    let mut conn = get_connection(state).await?;
    diesel::delete(allowed_users.filter(charger_id.eq(cid)))
        .execute(&mut conn)
        .await
        .map_err(|_| Error::InternalError)?;

    Ok(())
}

pub async fn delete_charger(
    charger: uuid::Uuid,
    state: &web::Data<AppState>,
) -> actix_web::Result<()> {
    use db_connector::schema::chargers::dsl::*;
    let mut conn = get_connection(state).await?;
    diesel::delete(chargers.filter(id.eq(charger)))
        .execute(&mut conn)
        .await
        .map_err(|_| Error::InternalError)?;

    Ok(())
}

pub async fn remove_charger_from_state(charger: uuid::Uuid, state: &web::Data<BridgeState<'_>>) {
    let socket = {
        let mut map = state.device_management_map_with_id.lock().await;
        map.remove(&charger)
    };

    if let Some(socket) = socket {
        let socket = socket.lock().await;
        let remote_address = socket.get_remote_address();
        let mut map = state.device_management_map.lock().await;
        let _ = map.remove(&remote_address);
    }
}

async fn is_last_user(cid: uuid::Uuid, state: &web::Data<AppState>) -> actix_web::Result<bool> {
    let mut conn = get_connection(state).await?;
    let count: i64 = {
        use db_connector::schema::allowed_users::dsl::*;

        allowed_users
            .filter(charger_id.eq(cid))
            .count()
            .get_result(&mut conn)
            .await
            .map_err(|_| Error::InternalError)?
    };

    Ok(count == 1)
}

pub async fn delete_keys_for_user(
    device_id: uuid::Uuid,
    uid: uuid::Uuid,
    state: &web::Data<AppState>,
) -> actix_web::Result<()> {
    let mut conn = get_connection(state).await?;
    {
        use db_connector::schema::wg_keys::dsl::*;
        diesel::delete(
            wg_keys
                .filter(user_id.eq(uid))
                .filter(charger_id.eq(device_id)),
        )
        .execute(&mut conn)
        .await
        .map_err(|_| Error::InternalError)?;
    }
    Ok(())
}

async fn delete_allowed_user(
    device_id: uuid::Uuid,
    uid: uuid::Uuid,
    state: &web::Data<AppState>,
) -> actix_web::Result<()> {
    let mut conn = get_connection(state).await?;
    {
        use db_connector::schema::allowed_users::dsl::*;
        diesel::delete(
            allowed_users
                .filter(user_id.eq(uid))
                .filter(charger_id.eq(device_id)),
        )
        .execute(&mut conn)
        .await
        .map_err(|_| Error::InternalError)?;
    }
    Ok(())
}

#[utoipa::path(
    context_path = "/charger",
    request_body = DeleteChargerSchema,
    responses(
        (status = 200, description = "Deletion was successful."),
        (status = 409, description = "The user sending the request is not the owner of the charger.")
    ),
    security(
        ("jwt" = [])
    )
)]
#[delete("/remove")]
pub async fn remove(
    state: web::Data<AppState>,
    user_id: crate::models::uuid::Uuid,
    data: web::Json<DeleteChargerSchema>,
    bridge_state: web::Data<BridgeState<'_>>,
) -> Result<impl Responder, actix_web::Error> {
    let device_id = parse_uuid(&data.charger)?;
    user_is_allowed(&state, user_id.clone().into(), device_id).await?;

    prompt_charger_to_remove_user(&bridge_state, device_id, user_id.clone().into()).await;
    if is_last_user(device_id, &state).await? {
        delete_all_keys(device_id, &state).await?;
        delete_all_allowed_users(device_id, &state).await?;

        let mut conn = get_connection(&state).await?;
        {
            use db_connector::schema::chargers::dsl as chargers;
            diesel::delete(chargers::chargers.filter(chargers::id.eq(device_id)))
                .execute(&mut conn)
                .await
                .map_err(|_| Error::InternalError)?;
        }
        delete_charger(device_id, &state).await?;
        remove_charger_from_state(device_id, &bridge_state).await;
    } else {
        let removed_user: uuid::Uuid = user_id.clone().into();
        delete_allowed_user(device_id, removed_user, &state).await?;
        delete_keys_for_user(device_id, removed_user, &state).await?;
    }

    Ok(HttpResponse::Ok())
}

#[cfg(test)]
pub(crate) mod tests {
    use core::panic;
    use std::str::FromStr;

    use super::*;
    use actix_web::{cookie::Cookie, test, App};
    use base64::{prelude::BASE64_STANDARD, Engine};
    use db_connector::test_connection_pool;
    use diesel_async::RunQueryDsl as _AsyncRunQueryDsl;

    use crate::{
        middleware::jwt::JwtMiddleware,
        routes::{
            charger::allow_user::UserAuth,
            user::tests::{get_test_uuid, random_positive_charger_id, TestUser},
        },
        tests::configure,
    };

    pub async fn remove_test_keys(mail: &str) -> anyhow::Result<()> {
        use db_connector::schema::wg_keys::dsl::*;

        let uid = get_test_uuid(mail).await?;
        let pool = test_connection_pool();
        let mut conn = pool.get().await.map_err(|e| anyhow::anyhow!("{e}"))?;
        diesel::delete(wg_keys.filter(user_id.eq(uid)))
            .execute(&mut conn)
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))?;
        Ok(())
    }

    pub async fn remove_allowed_test_users(uuid: &str) {
        use db_connector::schema::allowed_users::dsl::*;

        let device_id = uuid::Uuid::from_str(uuid).unwrap();
        let pool = test_connection_pool();
        let mut conn = pool.get().await.unwrap();
        diesel::delete(allowed_users.filter(charger_id.eq(device_id)))
            .execute(&mut conn)
            .await
            .unwrap();
    }

    pub async fn remove_test_device(device_id: &str) {
        let device_id = uuid::Uuid::from_str(device_id).unwrap();
        let pool = test_connection_pool();
        let mut conn = pool.get().await.unwrap();
        {
            use db_connector::schema::wg_keys::dsl as wg_keys;

            diesel::delete(wg_keys::wg_keys.filter(wg_keys::charger_id.eq(device_id)))
                .execute(&mut conn)
                .await
                .unwrap();
        }
        {
            use db_connector::schema::allowed_users::dsl as allowed_users;

            diesel::delete(
                allowed_users::allowed_users.filter(allowed_users::charger_id.eq(device_id)),
            )
            .execute(&mut conn)
            .await
            .unwrap();
        }
        {
            use db_connector::schema::chargers::dsl as chargers;
            diesel::delete(chargers::chargers.filter(chargers::id.eq(device_id)))
                .execute(&mut conn)
                .await
                .unwrap();
        }
    }

    async fn get_allowed_users_count(charger_id: uuid::Uuid) -> i64 {
        use db_connector::schema::allowed_users::dsl as allowed_users;

        let pool = test_connection_pool();
        let mut conn = pool.get().await.unwrap();
        allowed_users::allowed_users
            .filter(allowed_users::charger_id.eq(charger_id))
            .count()
            .get_result(&mut conn)
            .await
            .unwrap()
    }

    async fn get_wg_key_count(charger_id: uuid::Uuid) -> i64 {
        use db_connector::schema::wg_keys::dsl as wg_keys;

        let pool = test_connection_pool();
        let mut conn = pool.get().await.unwrap();
        wg_keys::wg_keys
            .filter(wg_keys::charger_id.eq(charger_id))
            .count()
            .get_result(&mut conn)
            .await
            .unwrap()
    }

    #[actix_web::test]
    async fn test_valid_remove() {
        let app = App::new()
            .configure(configure)
            .wrap(JwtMiddleware)
            .service(remove);
        let app = test::init_service(app).await;

        let (mut user, _) = TestUser::random().await;
        let token = user.login().await.to_owned();
        let device = user.add_random_charger().await;

        let schema = DeleteChargerSchema {
            charger: device.uuid.to_string(),
        };
        let req = test::TestRequest::delete()
            .uri("/remove")
            .cookie(Cookie::new("access_token", token))
            .set_json(schema)
            .to_request();
        match test::try_call_service(&app, req).await {
            Ok(resp) => {
                println!("{resp:?}");
                println!("{:?}", resp.response().body());
                assert!(resp.status().is_success());
            }
            Err(err) => {
                panic!("test valid delete failed: {:?}", err);
            }
        }

        let device_id = uuid::Uuid::from_str(&device.uuid).unwrap();

        assert_eq!(0, get_allowed_users_count(device_id).await);
        assert_eq!(0, get_wg_key_count(device_id).await);
    }

    #[actix_web::test]
    async fn test_valid_remove_with_allowed_user() {
        let app = App::new()
            .configure(configure)
            .wrap(JwtMiddleware)
            .service(remove);
        let app = test::init_service(app).await;

        let (user1, _) = TestUser::random().await;
        let (mut user2, _) = TestUser::random().await;
        let token = user2.login().await.to_owned();
        let device = user2.add_random_charger().await;
        user2
            .allow_user(
                &user1.mail,
                UserAuth::LoginKey(BASE64_STANDARD.encode(user1.get_login_key().await)),
                &device,
            )
            .await;

        let device_id = uuid::Uuid::from_str(&device.uuid).unwrap();
        let body = DeleteChargerSchema {
            charger: device.uuid,
        };
        let req = test::TestRequest::delete()
            .uri("/remove")
            .cookie(Cookie::new("access_token", token))
            .set_json(body)
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_success());

        assert_eq!(1, get_allowed_users_count(device_id).await);
        assert_eq!(5, get_wg_key_count(device_id).await);
    }

    #[actix_web::test]
    async fn test_unowned_charger_remove() {
        let app = App::new()
            .configure(configure)
            .wrap(JwtMiddleware)
            .service(remove);
        let app = test::init_service(app).await;

        let (mut user1, _) = TestUser::random().await;
        let email = user1.get_mail().to_owned();
        let (mut user2, _) = TestUser::random().await;
        let device_uid = random_positive_charger_id();
        user2.login().await;
        let device = user2.add_charger(device_uid).await;
        user2
            .allow_user(
                &email,
                UserAuth::LoginKey(BASE64_STANDARD.encode(user1.get_login_key().await)),
                &device,
            )
            .await;
        let token = user1.login().await;

        let body = DeleteChargerSchema {
            charger: device.uuid.clone(),
        };
        let req = test::TestRequest::delete()
            .uri("/remove")
            .set_json(body)
            .cookie(Cookie::new("access_token", token))
            .to_request();
        let resp = test::try_call_service(&app, req).await.unwrap();

        println!("{resp:?}");
        assert!(resp.status().is_success());

        let device_id = uuid::Uuid::from_str(&device.uuid).unwrap();
        assert_eq!(1, get_allowed_users_count(device_id).await);
        assert_eq!(5, get_wg_key_count(device_id).await);
    }

    #[actix_web::test]
    async fn test_not_allowed_charger_remove() {
        let app = App::new()
            .configure(configure)
            .wrap(JwtMiddleware)
            .service(remove);
        let app = test::init_service(app).await;

        let (mut user1, _) = TestUser::random().await;
        let (mut user2, _) = TestUser::random().await;
        let device_uid = random_positive_charger_id();
        user2.login().await;
        let device = user2.add_charger(device_uid).await;
        let token = user1.login().await;

        let body = DeleteChargerSchema {
            charger: device.uuid,
        };
        let req = test::TestRequest::delete()
            .uri("/remove")
            .set_json(body)
            .cookie(Cookie::new("access_token", token))
            .to_request();
        let resp = test::try_call_service(&app, req).await.unwrap();

        println!("{resp:?}");
        assert!(resp.status().is_client_error());
        assert!(resp.status().as_u16() == 401);
    }
}
