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

use actix_web::{get, web, HttpResponse, Responder};
use base64::{prelude::BASE64_STANDARD, Engine};
use db_connector::models::{allowed_users::AllowedUser, chargers::Charger};
use diesel::{prelude::*, result::Error::NotFound};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::{
    error::Error,
    routes::user::get_user,
    utils::{get_connection, web_block_unpacked},
    AppState, BridgeState,
};

#[derive(Serialize, Deserialize, ToSchema)]
pub enum ChargerStatus {
    Disconnected = 0,
    Connected = 1,
}

#[derive(Serialize, Deserialize, ToSchema)]
pub struct GetChargerSchema {
    id: String,
    uid: i32,
    name: String,
    note: Option<String>,
    status: ChargerStatus,
    port: i32,
    valid: bool,
    last_state_change: Option<i64>,
    firmware_version: String,
}

/// Get all chargers that the current user has access to.
#[utoipa::path(
    context_path = "/charger",
    responses(
        (status = 200, description = "Success", body = [GetChargerSchema]),
        (status = 400, description = "Somehow got a valid jwt but the user does not exist.")
    ),
    security(
        ("jwt" = [])
    )
)]
#[get("/get_chargers")]
pub async fn get_chargers(
    state: web::Data<AppState>,
    uid: crate::models::uuid::Uuid,
    bridge_state: web::Data<BridgeState>,
) -> Result<impl Responder, actix_web::Error> {
    use db_connector::schema::allowed_users::dsl as allowed_users;
    use db_connector::schema::chargers::dsl as chargers;

    let user = get_user(&state, uid.into()).await?;

    let mut conn = get_connection(&state)?;
    let charger: Vec<(Charger, AllowedUser)> = web_block_unpacked(move || {
        let allowed_users: Vec<AllowedUser> = match AllowedUser::belonging_to(&user)
            .select(AllowedUser::as_select())
            .load(&mut conn)
        {
            Ok(d) => d,
            Err(NotFound) => Vec::new(),
            Err(err) => {
                log::error!("Failed to load allowed users: {err}");
                return Err(Error::InternalError);
            }
        };
        let charger_ids = AllowedUser::belonging_to(&user).select(allowed_users::charger_id);
        let chargers: Vec<Charger> = match chargers::chargers
            .filter(chargers::id.eq_any(charger_ids))
            .select(Charger::as_select())
            .load(&mut conn)
        {
            Ok(v) => v,
            Err(err) => {
                log::error!("Failed to load chargers: {err}");
                return Err(Error::InternalError);
            }
        };

        let chargers_by_users: Vec<(Charger, AllowedUser)> = allowed_users
            .grouped_by(&chargers)
            .into_iter()
            .zip(chargers)
            .filter_map(|(allowed_users, charger)| {
                allowed_users.first().map(|au| (charger, au.clone()))
            })
            .collect();

        Ok(chargers_by_users)
    })
    .await?;

    let charger_map = bridge_state.charger_management_map_with_id.lock().await;
    let charger = charger
        .into_iter()
        .map(|(c, allowed_user)| {
            let status = if charger_map.contains_key(&c.id) {
                ChargerStatus::Connected
            } else {
                ChargerStatus::Disconnected
            };

            let name = if let Some(name) = allowed_user.name {
                name
            } else if let Some(name) = c.name {
                BASE64_STANDARD.encode(name)
            } else {
                String::new()
            };

            GetChargerSchema {
                id: c.id.to_string(),
                uid: c.uid,
                name,
                note: allowed_user.note,
                status,
                port: c.webinterface_port,
                valid: allowed_user.valid,
                last_state_change: c.last_state_change.map(|ts| ts.and_utc().timestamp()),
                firmware_version: c.firmware_version,
            }
        })
        .collect::<Vec<GetChargerSchema>>();

    Ok(HttpResponse::Ok().json(charger))
}

#[cfg(test)]
mod tests {
    use actix_web::{cookie::Cookie, test, App};
    use base64::{prelude::BASE64_STANDARD, Engine};
    use db_connector::test_connection_pool;
    use diesel::prelude::*;
    use rand::TryRngCore;
    use rand_core::OsRng;
    use std::str::FromStr;

    use super::*;
    use crate::{
        middleware::jwt::JwtMiddleware,
        routes::{charger::allow_user::UserAuth, user::tests::TestUser},
        tests::configure,
    };

    /// Test if only the chargers the user has access to will be returned.
    #[actix_web::test]
    async fn test_get_chargers() {
        let (mut user1, _) = TestUser::random().await;
        let (mut user2, _) = TestUser::random().await;
        user1.login().await;
        user2.login().await;
        for _ in 0..5 {
            let _ = user1.add_random_charger().await;
            let charger = user2.add_random_charger().await;
            user2
                .allow_user(
                    &user1.mail,
                    UserAuth::LoginKey(BASE64_STANDARD.encode(user1.get_login_key().await)),
                    &charger,
                )
                .await;
        }
        for _ in 0..5 {
            let uuid = OsRng.try_next_u32().unwrap() as i32;
            user2.add_charger(uuid).await;
        }

        let app = App::new()
            .configure(configure)
            .wrap(JwtMiddleware)
            .service(get_chargers);
        let app = test::init_service(app).await;

        println!("Access token: {}", user1.get_access_token());
        let req = test::TestRequest::get()
            .uri("/get_chargers")
            .cookie(Cookie::new("access_token", user1.get_access_token()))
            .to_request();
        let resp: Vec<GetChargerSchema> = test::call_and_read_body_json(&app, req).await;
        assert_eq!(resp.len(), 10);
    }

    #[actix_web::test]
    async fn test_get_not_existing_chargers() {
        let (mut user1, _) = TestUser::random().await;
        user1.login().await;

        let app = App::new()
            .configure(configure)
            .wrap(JwtMiddleware)
            .service(get_chargers);
        let app = test::init_service(app).await;

        let req = test::TestRequest::get()
            .uri("/get_chargers")
            .cookie(Cookie::new("access_token", user1.get_access_token()))
            .to_request();
        let resp: Vec<GetChargerSchema> = test::call_and_read_body_json(&app, req).await;
        assert_eq!(resp.len(), 0);
    }

    /// Test that the race condition between fetching allowed_users and chargers is handled.
    ///
    /// This tests the fix for a race condition where:
    /// 1. allowed_users are fetched for a user
    /// 2. chargers are fetched based on those allowed_users
    /// 3. Between those two queries, an allowed_user entry could be deleted
    /// 4. This would result in grouped_by returning an empty array for that charger
    ///
    /// The fix uses filter_map with .first() instead of directly accessing [0],
    /// which would panic on an empty array.
    #[actix_web::test]
    async fn test_race_condition_allowed_user_deleted_between_queries() {
        let (mut user1, _) = TestUser::random().await;
        user1.login().await;

        // Add two chargers - we'll delete the allowed_user entry for one of them
        let charger1 = user1.add_random_charger().await;
        let charger2 = user1.add_random_charger().await;

        // Directly delete the allowed_user entry for charger1 from the database
        // This simulates the race condition where another request deletes the
        // allowed_user between fetching allowed_users and joining with chargers
        {
            use db_connector::schema::allowed_users::dsl::*;

            let charger1_uuid = uuid::Uuid::from_str(&charger1.uuid).unwrap();
            let pool = test_connection_pool();
            let mut conn = pool.get().unwrap();

            diesel::delete(allowed_users.filter(charger_id.eq(charger1_uuid)))
                .execute(&mut conn)
                .expect("Failed to delete allowed_user entry");
        }

        let app = App::new()
            .configure(configure)
            .wrap(JwtMiddleware)
            .service(get_chargers);
        let app = test::init_service(app).await;

        // This should not panic even though charger1 has no allowed_users entry
        let req = test::TestRequest::get()
            .uri("/get_chargers")
            .cookie(Cookie::new("access_token", user1.get_access_token()))
            .to_request();
        let resp: Vec<GetChargerSchema> = test::call_and_read_body_json(&app, req).await;

        // Only charger2 should be returned since charger1's allowed_user was deleted
        assert_eq!(resp.len(), 1);
        assert_eq!(resp[0].id, charger2.uuid);
    }
}
