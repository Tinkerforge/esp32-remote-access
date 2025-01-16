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

use crate::{
    error::Error,
    utils::{get_connection, web_block_unpacked},
    AppState,
};
use actix_web::{put, web, HttpResponse, Responder};
use db_connector::models::users::User;
use diesel::{prelude::*, result::Error::NotFound};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use validator::Validate;

#[derive(Serialize, Deserialize, ToSchema, Validate)]
pub struct UpdateUserSchema {
    #[validate(length(min = 3))]
    pub name: String,
    #[validate(email)]
    pub email: String,
}

/// Update basic user information.
#[utoipa::path(
    context_path = "/user",
    request_body = UpdateUserSchema,
    responses(
        (status = 200, description = "Update was successful.")
    ),
    security(
        ("jwt" = [])
    )
)]
#[put("/update_user")]
pub async fn update_user(
    state: web::Data<AppState>,
    user: actix_web_validator::Json<UpdateUserSchema>,
    uid: crate::models::uuid::Uuid,
) -> Result<impl Responder, actix_web::Error> {
    use db_connector::schema::users::dsl::*;

    let mut conn = get_connection(&state)?;
    web_block_unpacked(move || {
        match users
            .filter(email.eq(&user.email))
            .select(User::as_select())
            .get_result(&mut conn) as Result<User, diesel::result::Error>
        {
            Err(NotFound) => (),
            Ok(u) => {
                if u.id != uid.clone().into() {
                    return Err(Error::UserAlreadyExists);
                }
            }
            Err(_err) => return Err(Error::InternalError),
        }

        match diesel::update(users.find::<uuid::Uuid>(uid.clone().into()))
            .set(email.eq(&user.email))
            .execute(&mut conn)
        {
            Ok(_) => (),
            Err(NotFound) => return Err(Error::Unauthorized),
            Err(_err) => return Err(Error::InternalError),
        }

        match diesel::update(users.find::<uuid::Uuid>(uid.into()))
            .set(name.eq(&user.name))
            .execute(&mut conn)
        {
            Ok(_) => (),
            Err(NotFound) => return Err(Error::Unauthorized),
            Err(_err) => return Err(Error::InternalError),
        }

        Ok(())
    })
    .await?;

    Ok(HttpResponse::Ok())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        defer,
        routes::{
            auth::{
                login::tests::verify_and_login_user,
                register::tests::{create_user, delete_user},
            },
            user::{
                me::tests::get_test_user,
                tests::TestUser,
            },
        },
        tests::configure,
    };
    use actix_web::{cookie::Cookie, test, App};

    #[actix_web::test]
    async fn test_update_email() {
        let mail = "update_mail@test.invalid";
        let key = create_user(mail).await;
        defer!(delete_user(mail));
        let update_mail = format!("t{}", mail);
        defer!(delete_user(&update_mail));

        let app = App::new()
            .configure(configure)
            .service(update_user)
            .wrap(crate::middleware::jwt::JwtMiddleware);
        let app = test::init_service(app).await;

        let user = get_test_user(mail);
        let mut user = user;
        user.email = update_mail.clone();
        let user = UpdateUserSchema {
            name: user.name,
            email: user.email,
        };

        let (token, _) = verify_and_login_user(mail, key).await;
        let req = test::TestRequest::put()
            .uri("/update_user")
            .set_json(user)
            .cookie(Cookie::new("access_token", token))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_success());

        let _ = get_test_user(&update_mail);
    }

    #[actix_web::test]
    async fn test_existing_email() {
        let (mut user, mail) = TestUser::random().await;
        let (_user2, mail2) = TestUser::random().await;
        let token = user.login().await;
        let app = App::new()
            .configure(configure)
            .service(update_user)
            .wrap(crate::middleware::jwt::JwtMiddleware);
        let app = test::init_service(app).await;

        let user = get_test_user(&mail);
        let mut user =user;
        user.email = mail2;
        let user = UpdateUserSchema {
            name: user.name,
            email: user.email,
        };

        let req = test::TestRequest::put()
            .uri("/update_user")
            .set_json(user)
            .cookie(Cookie::new("access_token", token))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_client_error());
    }
}
