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

use actix_web::{put, web, HttpResponse, Responder};
use db_connector::models::{allowed_users::AllowedUser, wg_keys::WgKey};
use diesel::prelude::*;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::{
    error::Error,
    routes::{auth::login::FindBy, user::get_uuid},
    utils::{get_connection, web_block_unpacked},
    AppState,
};

#[derive(Debug, Deserialize, Serialize, ToSchema, Clone)]
pub struct AllowUserSchema {
    charger_id: i32,
    pass: String,
    email: String,
    wg_keys: [super::add::Keys; 5],
    key: Vec<u8>,
    charger_name: Vec<u8>,
    note: Vec<u8>,
}

async fn add_keys(state: &web::Data<AppState>, keys: [super::add::Keys; 5], uid: uuid::Uuid, cid: i32) -> actix_web::Result<()> {
    let mut conn = get_connection(&state)?;
    web_block_unpacked(move || {
        use db_connector::schema::wg_keys::dsl::*;

        let insert_keys: Vec<WgKey> = keys.into_iter().map(|key| {
            WgKey {
                id: uuid::Uuid::new_v4(),
                user_id: uid,
                charger_id: cid,
                in_use: false,
                charger_pub: key.charger_public,
                web_private: key.web_private,
                psk: key.psk,
                web_address: key.web_address,
                charger_address: key.charger_address,
                connection_no: key.connection_no as i32
            }
        }).collect();

        match diesel::insert_into(wg_keys).values(&insert_keys).execute(&mut conn) {
            Ok(_) => Ok(()),
            Err(_err) => Err(Error::InternalError)
        }
    }).await?;

    Ok(())
}

/// Give another user permission to access a charger owned by the user.
#[utoipa::path(
    context_path = "/charger",
    request_body = AllowUserSchema,
    responses(
        (status = 200, description = "Allowing the user to access the charger was successful."),
        (status = 400, description = "The user does not exist.")
    )
)]
#[put("/allow_user")]
pub async fn allow_user(
    state: web::Data<AppState>,
    allow_user: web::Json<AllowUserSchema>,
) -> Result<impl Responder, actix_web::Error> {
    use db_connector::schema::allowed_users::dsl::*;

    let allowed_uuid = get_uuid(&state, FindBy::Email(allow_user.email.clone())).await?;
    let mut conn = get_connection(&state)?;
    let allow_user = allow_user.clone();
    web_block_unpacked(move || {
        let u = AllowedUser {
            id: uuid::Uuid::new_v4(),
            user_id: allowed_uuid,
            charger_id: allow_user.charger_id,
            valid: false,
            key: Some(allow_user.key),
            name: Some(allow_user.charger_name),
            note: Some(allow_user.note)
        };

        match diesel::insert_into(allowed_users)
            .values(u)
            .execute(&mut conn)
        {
            Ok(_) => Ok(()),
            Err(_err) => Err(Error::InternalError),
        }
    })
    .await?;

    match add_keys(&state, allow_user.wg_keys, allowed_uuid, allow_user.charger_id).await {
        Ok(_) => (),
        Err(_err) => {

        }
    }

    Ok(HttpResponse::Ok())
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use actix_web::{cookie::Cookie, test, App};
    use rand::RngCore;
    use rand_core::OsRng;

    use crate::{middleware::jwt::JwtMiddleware, routes::{charger::add::tests::generate_random_keys, user::tests::TestUser}, tests::configure};

    pub async fn add_allowed_test_user(email: &str, charger_id: i32, token: &str) {
        let app = App::new()
            .configure(configure)
            .wrap(JwtMiddleware)
            .service(allow_user);
        let app = test::init_service(app).await;

        let body = AllowUserSchema {
            charger_id,
            email: email.to_string(),
            pass: String::new(),
            wg_keys: generate_random_keys(),
            key: Vec::new(),
            charger_name: Vec::new(),
            note: Vec::new(),
        };
        let req = test::TestRequest::put()
            .cookie(Cookie::new("access_token", token))
            .uri("/allow_user")
            .set_json(body)
            .to_request();
        let resp = test::call_service(&app, req).await;
        println!("{}", resp.status());
        println!("{:?}", resp.response().body());
        assert!(resp.status().is_success());
    }

    #[actix_web::test]
    async fn test_allow_users() {
        let _user2 = TestUser::random().await;
        let (mut user1, _) = TestUser::random().await;
        let email = user1.get_mail().to_string();

        let charger = OsRng.next_u32() as i32;
        let token = user1.login().await.to_string();
        user1.add_charger(charger).await;

        let app = App::new()
            .configure(configure)
            .wrap(JwtMiddleware)
            .service(allow_user);
        let app = test::init_service(app).await;

        let allow = AllowUserSchema {
            charger_id: charger,
            email,
            pass: String::new(),
            wg_keys: generate_random_keys(),
            key: Vec::new(),
            charger_name: Vec::new(),
            note: Vec::new(),
        };
        let req = test::TestRequest::put()
            .uri("/allow_user")
            .cookie(Cookie::new("access_token", token))
            .set_json(allow)
            .to_request();
        let resp = test::call_service(&app, req).await;

        assert!(resp.status().is_success());
    }

    #[actix_web::test]
    async fn test_allow_users_non_existing() {
        let (mut user, _) = TestUser::random().await;

        let charger = OsRng.next_u32() as i32;
        let token = user.login().await.to_string();
        user.add_charger(charger).await;

        let app = App::new()
            .configure(configure)
            .wrap(JwtMiddleware)
            .service(allow_user);
        let app = test::init_service(app).await;

        let allow = AllowUserSchema {
            charger_id: charger,
            email: uuid::Uuid::new_v4().to_string(),
            pass: String::new(),
            wg_keys: generate_random_keys(),
            key: Vec::new(),
            charger_name: Vec::new(),
            note: Vec::new(),
        };
        let req = test::TestRequest::put()
            .uri("/allow_user")
            .cookie(Cookie::new("access_token", token))
            .set_json(allow)
            .to_request();
        let resp = test::call_service(&app, req).await;

        assert!(resp.status().is_client_error());
    }
}
