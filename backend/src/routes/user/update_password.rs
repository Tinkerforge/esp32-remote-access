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
    let _ = validate_password(
        &data.old_login_key,
        FindBy::Uuid(uid.clone().into()),
        conn,
        &state.hasher,
    )
    .await?;

    let new_hash = match hash_key(data.new_login_key.clone(), &state.hasher).await {
        Ok(hash) => hash,
        Err(_err) => return Err(Error::InternalError.into()),
    };

    let mut conn = get_connection(&state)?;
    match web::block(move || {
        match diesel::update(users.find::<uuid::Uuid>(uid.into()))
            .set((
                login_key.eq(new_hash),
                secret_nonce.eq(&data.new_secret_nonce),
                secret.eq(&data.new_encrypted_secret),
                secret_salt.eq(&data.new_secret_salt),
                login_salt.eq(&data.new_login_salt),
            ))
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
    use libsodium_sys::{
        crypto_box_SECRETKEYBYTES, crypto_secretbox_KEYBYTES, crypto_secretbox_MACBYTES,
        crypto_secretbox_NONCEBYTES, crypto_secretbox_easy, crypto_secretbox_open_easy,
    };

    use crate::{
        routes::{
            auth::get_login_salt::tests::get_test_login_salt,
            user::{
                get_secret::tests::get_test_secret,
                tests::{generate_random_bytes_len, hash_test_key, TestUser},
            },
        },
        tests::configure,
        utils::generate_random_bytes,
    };

    #[actix_web::test]
    async fn test_valid_password_update() {
        let (mut user, mail) = TestUser::random().await;
        let token = user.login().await.to_owned();

        let login_salt = get_test_login_salt(&mail).await;
        let login_key = hash_test_key(&user.password, &login_salt, None);
        let secret_data = get_test_secret(&token).await;
        let secret_key = hash_test_key(
            &user.password,
            &secret_data.secret_salt,
            Some(crypto_secretbox_KEYBYTES as usize),
        );
        let mut secret = vec![0u8; crypto_box_SECRETKEYBYTES as usize];
        unsafe {
            if crypto_secretbox_open_easy(
                secret.as_mut_ptr(),
                secret_data.secret.as_ptr(),
                secret_data.secret.len() as u64,
                secret_data.secret_nonce.as_ptr(),
                secret_key.as_ptr(),
            ) != 0
            {
                panic!("Decrypting secret failed.");
            }
        }

        let new_password = generate_random_bytes_len(48);
        let new_login_salt = generate_random_bytes_len(48);
        let new_secret_salt = generate_random_bytes_len(48);
        let new_secret_nonce = generate_random_bytes_len(crypto_secretbox_NONCEBYTES as usize);
        let new_login_key = hash_test_key(&new_password, &new_login_salt, None);
        let new_secret_key = hash_test_key(
            &new_password,
            &new_secret_salt,
            Some(crypto_secretbox_KEYBYTES as usize),
        );
        let mut new_encrypted_secret =
            vec![0u8; (crypto_secretbox_MACBYTES + crypto_secretbox_KEYBYTES) as usize];
        unsafe {
            if crypto_secretbox_easy(
                new_encrypted_secret.as_mut_ptr(),
                secret.as_ptr(),
                crypto_box_SECRETKEYBYTES as u64,
                new_secret_nonce.as_ptr(),
                new_secret_key.as_ptr(),
            ) != 0
            {
                panic!("Encrypted secret failed.");
            }
        }

        let app = App::new()
            .configure(configure)
            .service(update_password)
            .wrap(crate::middleware::jwt::JwtMiddleware);
        let app = test::init_service(app).await;

        let data = PasswordUpdateSchema {
            old_login_key: login_key,
            new_login_key,
            new_login_salt,
            new_secret_nonce,
            new_secret_salt,
            new_encrypted_secret,
        };

        let req = test::TestRequest::put()
            .uri("/update_password")
            .cookie(Cookie::new("access_token", token))
            .set_json(data)
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_success());
        user.password = new_password;
        let _ = user.additional_login().await;
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
        let hasher = crate::hasher::HasherManager::default();
        assert!(
            validate_password(&new_key, FindBy::Email(mail), conn, &hasher)
                .await
                .is_err()
        );
    }
}
