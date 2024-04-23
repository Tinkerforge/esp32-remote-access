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

use crate::{error::Error, models::filtered_user::FilteredUser, utils::{get_connection, web_block_unpacked}, AppState};
use actix_web::{put, web, HttpResponse, Responder};
use db_connector::models::users::User;
use diesel::{prelude::*, result::Error::NotFound};

/// Update basic user information.
#[utoipa::path(
    context_path = "/user",
    request_body = FilteredUser,
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
    user: actix_web_validator::Json<FilteredUser>,
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

        match users.filter(name.eq(&user.name))
            .select(User::as_select())
            .get_result(&mut conn) {
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
    }).await?;


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
            user::{me::tests::{get_test_user, get_test_user_by_email}, tests::TestUser},
        },
        tests::configure,
    };
    use actix_web::{cookie::Cookie, test, App};

    #[actix_web::test]
    async fn test_update_email() {
        let mail = "update_mail@test.invalid";
        let username = "update_mail_user";
        let key = create_user(mail, username).await;
        defer!(delete_user(mail));
        let update_mail = format!("t{}", mail);
        defer!(delete_user(&update_mail));

        let app = App::new()
            .configure(configure)
            .service(update_user)
            .wrap(crate::middleware::jwt::JwtMiddleware);
        let app = test::init_service(app).await;

        let user = get_test_user_by_email(mail);
        let mut user = FilteredUser::from(user);
        user.email = update_mail.clone();

        let (token, _) = verify_and_login_user(username, key).await;
        let req = test::TestRequest::put()
            .uri("/update_user")
            .set_json(user)
            .cookie(Cookie::new("access_token", token))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_success());

        let _ = get_test_user_by_email(&update_mail);
    }

    #[actix_web::test]
    async fn test_existing_username() {
        let (mut user, username) = TestUser::random().await;
        let (_user2, username2) = TestUser::random().await;
        let token = user.login().await;
        let app = App::new()
            .configure(configure)
            .service(update_user)
            .wrap(crate::middleware::jwt::JwtMiddleware);
        let app = test::init_service(app).await;

        let user = get_test_user(&username);
        let mut user = FilteredUser::from(user);
        user.name = username2;
        let req = test::TestRequest::put()
            .uri("/update_user")
            .set_json(user)
            .cookie(Cookie::new("access_token", token))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_client_error());
    }
}
