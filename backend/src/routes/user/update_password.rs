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
use diesel::prelude::*;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use validator::Validate;

use crate::{
    error::Error,
    routes::auth::{
        login::{validate_password, FindBy},
        register::hash_key,
    },
    utils::get_connection,
    AppState,
};

#[derive(Validate, Deserialize, Serialize, ToSchema)]
pub struct PasswordUpdateSchema {
    #[schema(value_type = Vec<u32>)]
    old_login_key: Vec<u8>,
    #[schema(value_type = Vec<u32>)]
    new_login_key: Vec<u8>,
    #[schema(value_type = Vec<u32>)]
    new_login_salt: Vec<u8>,
    #[schema(value_type = Vec<u32>)]
    new_secret_nonce: Vec<u8>,
    #[schema(value_type = Vec<u32>)]
    new_secret_salt: Vec<u8>,
    #[schema(value_type = Vec<u32>)]
    new_encrypted_secret: Vec<u8>,
}

/// Update the user password
#[utoipa::path(
    context_path = "/user",
    request_body = PasswordUpdateSchema,
    responses(
        (status = 200, description = "Password update was successful."),
        (status = 400, description = "The old password was wrong.")
    ),
    security(
        ("jwt" = [])
    )
)]
#[put("/update_password")]
pub async fn update_password(
    state: web::Data<AppState>,
    uid: crate::models::uuid::Uuid,
    data: actix_web_validator::Json<PasswordUpdateSchema>,
) -> Result<impl Responder, actix_web::Error> {
    use db_connector::schema::users::dsl::*;

    let conn = get_connection(&state)?;
    let _ = validate_password(&data.old_login_key, FindBy::Uuid(uid.clone().into()), conn).await?;

    let new_hash = match hash_key(&data.new_login_key) {
        Ok(hash) => hash,
        Err(_err) => return Err(Error::InternalError.into()),
    };

    let mut conn = get_connection(&state)?;
    match web::block(move || {
        match diesel::update(users.find::<uuid::Uuid>(uid.into()))
            .set((login_key.eq(new_hash), secret_nonce.eq(&data.new_secret_nonce), secret.eq(&data.new_encrypted_secret), secret_salt.eq(&data.new_secret_salt), login_salt.eq(&data.new_login_salt)))
            .execute(&mut conn)
        {
            Ok(_) => (),
            Err(_err) => return Err(Error::InternalError),
        }

        Ok(())
    })
    .await
    {
        Ok(res) => match res {
            Ok(()) => Ok(HttpResponse::Ok()),
            Err(err) => Err(err.into()),
        },
        Err(_err) => Err(Error::InternalError.into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::{cookie::Cookie, test, App};

    use crate::{
        defer,
        routes::{
            auth::{
                login::tests::{login_user, verify_and_login_user},
                register::tests::{create_user, delete_user},
            },
            user::tests::TestUser,
        },
        tests::configure,
        utils::generate_random_bytes,
    };

    #[actix_web::test]
    async fn test_valid_password_update() {
        let mail = "valid_password_update@test.invalid";
        let key = create_user(mail).await;
        defer!(delete_user(mail));
        let (token, _) = verify_and_login_user(mail, key.clone()).await;

        let app = App::new()
            .configure(configure)
            .service(update_password)
            .wrap(crate::middleware::jwt::JwtMiddleware);
        let app = test::init_service(app).await;

        let new_key: Vec<u8> = generate_random_bytes();
        let data = PasswordUpdateSchema {
            old_login_key: key,
            new_login_key: new_key.clone(),
            new_login_salt: generate_random_bytes(),
            new_secret_nonce: generate_random_bytes(),
            new_secret_salt: generate_random_bytes(),
            new_encrypted_secret: generate_random_bytes(),
        };

        let req = test::TestRequest::put()
            .uri("/update_password")
            .cookie(Cookie::new("access_token", token))
            .set_json(data)
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_success());

        let _ = login_user(mail, new_key).await;
    }

    #[actix_web::test]
    async fn test_invalid_old_password() {
        let (mut user, mail) = TestUser::random().await;
        let token = user.login().await;

        let app = App::new()
            .configure(configure)
            .service(update_password)
            .wrap(crate::middleware::jwt::JwtMiddleware);
        let app = test::init_service(app).await;

        let new_key = generate_random_bytes();
        let data = PasswordUpdateSchema {
            old_login_key: generate_random_bytes(),
            new_login_key: new_key.clone(),
            new_login_salt: generate_random_bytes(),
            new_secret_nonce: generate_random_bytes(),
            new_secret_salt: generate_random_bytes(),
            new_encrypted_secret: generate_random_bytes(),
        };
        let req = test::TestRequest::put()
            .uri("/update_password")
            .cookie(Cookie::new("access_token", token))
            .set_json(data)
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_client_error());

        let pool = db_connector::test_connection_pool();
        let conn = pool.get().unwrap();
        assert!(
            validate_password(&new_key, FindBy::Email(mail), conn)
                .await
                .is_err()
        );
    }
}
