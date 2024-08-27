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

#[derive(Serialize, Deserialize)]
enum ChargerStatus {
    Disconnected = 0,
    Connected = 1,
}

#[derive(Serialize, Deserialize, ToSchema)]
pub struct GetChargerSchema {
    id: i32,
    name: Option<Vec<u8>>,
    status: ChargerStatus,
    port: i32,
    valid: bool,
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
            Err(_err) => return Err(Error::InternalError),
        };
        let charger_ids = AllowedUser::belonging_to(&user).select(allowed_users::charger_id);
        let chargers: Vec<Charger> = match chargers::chargers
            .filter(chargers::id.eq_any(charger_ids))
            .select(Charger::as_select())
            .load(&mut conn)
        {
            Ok(v) => v,
            Err(_err) => return Err(Error::InternalError),
        };

        let chargers_by_users: Vec<(Charger, AllowedUser)> = allowed_users
            .grouped_by(&chargers)
            .into_iter()
            .zip(chargers)
            .map(|(allowed_users, charger)| (charger, allowed_users[0].clone()))
            .collect();

        Ok(chargers_by_users)
    })
    .await?;

    let charger_map = bridge_state.charger_management_map_with_id.lock().unwrap();
    let charger = charger
        .into_iter()
        .map(|(c, allowed_user)| {
            let status = if charger_map.contains_key(&c.id) {
                ChargerStatus::Connected
            } else {
                ChargerStatus::Disconnected
            };

            GetChargerSchema {
                id: c.id,
                name: c.name,
                status,
                port: c.webinterface_port,
                valid: allowed_user.valid,
            }
        })
        .collect::<Vec<GetChargerSchema>>();

    Ok(HttpResponse::Ok().json(charger))
}

#[cfg(test)]
mod tests {
    use actix_web::{cookie::Cookie, test, App};
    use rand::RngCore;
    use rand_core::OsRng;

    use super::*;
    use crate::{middleware::jwt::JwtMiddleware, routes::user::tests::TestUser, tests::configure};

    /// Test if only the chargers the user has access to will be returned.
    #[actix_web::test]
    async fn test_get_chargers() {
        let mut owned_chargers: Vec<i32> = Vec::new();
        let mut accessable_chargers: Vec<i32> = Vec::new();
        let (mut user1, _) = TestUser::random().await;
        let email = user1.get_mail().to_owned();
        let (mut user2, _) = TestUser::random().await;
        user1.login().await;
        user2.login().await;
        for _ in 0..5 {
            let uuid1 = OsRng.next_u32() as i32;
            let uuid2 = OsRng.next_u32() as i32;
            user1.add_charger(uuid1).await;
            user2.add_charger(uuid2).await;
            user2.allow_user(&email, uuid2).await;
            owned_chargers.push(uuid1);
            accessable_chargers.push(uuid2);
        }
        for _ in 0..5 {
            let uuid = OsRng.next_u32() as i32;
            user2.add_charger(uuid).await;
        }

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
}
