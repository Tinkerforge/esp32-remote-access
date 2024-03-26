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
use db_connector::models::allowed_users::AllowedUser;
use diesel::prelude::*;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::{
    error::Error,
    routes::{charger::charger_belongs_to_user, user::get_uuid_from_email},
    utils::{get_connection, web_block_unpacked},
    AppState,
};

#[derive(Debug, Deserialize, Serialize, ToSchema)]
pub struct AllowUserSchema {
    charger_id: i32,
    user_mail: String,
}

/// Give another user permission to access a charger owned by the user.
#[utoipa::path(
    context_path = "/charger",
    request_body = AllowUserSchema,
    responses(
        (status = 200, description = "Allowing the user to access the charger was successful."),
        (status = 400, description = "The user does not exist.")
    ),
    security(
        ("jwt" = [])
    )
)]
#[put("/allow_user")]
pub async fn allow_user(
    state: web::Data<AppState>,
    uid: crate::models::uuid::Uuid,
    allow_user: web::Json<AllowUserSchema>,
) -> Result<impl Responder, actix_web::Error> {
    use db_connector::schema::allowed_users::dsl::*;

    if !charger_belongs_to_user(&state, uid.into(), allow_user.charger_id).await? {
        return Err(Error::UserIsNotOwner.into());
    }

    let allowed_uuid = get_uuid_from_email(&state, allow_user.user_mail.clone()).await?;
    let mut conn = get_connection(&state)?;
    web_block_unpacked(move || {
        let u = AllowedUser {
            id: uuid::Uuid::new_v4(),
            user_id: allowed_uuid,
            charger_id: allow_user.charger_id,
            is_owner: false,
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

    Ok(HttpResponse::Ok())
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use actix_web::{cookie::Cookie, test, App};
    use rand::RngCore;
    use rand_core::OsRng;

    use crate::{middleware::jwt::JwtMiddleware, routes::user::tests::TestUser, tests::configure};

    pub async fn add_allowed_test_user(user_mail: &str, charger_id: i32, token: &str) {
        let app = App::new()
            .configure(configure)
            .wrap(JwtMiddleware)
            .service(allow_user);
        let app = test::init_service(app).await;

        let body = AllowUserSchema {
            charger_id,
            user_mail: user_mail.to_string(),
        };
        let req = test::TestRequest::put()
            .cookie(Cookie::new("access_token", token))
            .uri("/allow_user")
            .set_json(body)
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_success());
    }

    #[actix_web::test]
    async fn test_allow_users() {
        let mail1 = "allow_user1@test.invalid";
        let mail2 = "allow_user2@test.invalid";

        let _user2 = TestUser::new(mail2).await;
        let mut user1 = TestUser::new(mail1).await;

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
            user_mail: mail2.to_string(),
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
        let mail1 = "allow_user_non_existing1@test.invalid";
        let mail2 = "allow_user_non_existing2@test.invalid";

        let mut user1 = TestUser::new(mail1).await;

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
            user_mail: mail2.to_string(),
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
