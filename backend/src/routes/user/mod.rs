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

pub mod delete;
pub mod get_secret;
pub mod logout;
pub mod me;
pub mod update_password;
pub mod update_user;
pub mod create_authorization_token;
pub mod get_authorization_tokens;
pub mod delete_authorization_token;

use crate::{
    error::Error,
    middleware::jwt::JwtMiddleware,
    utils::{get_connection, web_block_unpacked},
    AppState,
};
use actix_web::web::{self, ServiceConfig};
use db_connector::models::users::User;
use diesel::{prelude::*, result::Error::NotFound, ExpressionMethods};

use super::auth::login::FindBy;

pub fn configure(cfg: &mut ServiceConfig) {
    let scope = web::scope("/user")
        .wrap(JwtMiddleware)
        .service(update_user::update_user)
        .service(update_password::update_password)
        .service(get_secret::get_secret)
        .service(logout::logout)
        .service(delete::delete_user)
        .service(create_authorization_token::create_authorization_token)
        .service(get_authorization_tokens::get_authorization_tokens)
        .service(delete_authorization_token::delete_authorization_token)
        .service(me::me);
    cfg.service(scope);
}

/**
 * Lookup the corresponding Uuid for an email or username or check if uuid exists.
 */
pub async fn get_user_id(
    state: &web::Data<AppState>,
    find: FindBy,
) -> Result<uuid::Uuid, actix_web::Error> {
    use db_connector::schema::users::dsl::*;

    let mut conn = get_connection(state)?;
    web_block_unpacked(move || {
        let user: User = match find {
            FindBy::Email(mail) => {
                match users
                    .filter(email.eq(mail))
                    .select(User::as_select())
                    .get_result(&mut conn)
                {
                    Ok(user) => user,
                    Err(NotFound) => return Err(Error::UserDoesNotExist),
                    Err(_err) => return Err(Error::InternalError),
                }
            }
            FindBy::Username(username) => {
                match users
                    .filter(name.eq(username))
                    .select(User::as_select())
                    .get_result(&mut conn)
                {
                    Ok(user) => user,
                    Err(NotFound) => return Err(Error::UserDoesNotExist),
                    Err(_err) => return Err(Error::InternalError),
                }
            }
            FindBy::Uuid(uuid) => {
                match users
                    .find(uuid)
                    .select(User::as_select())
                    .get_result(&mut conn)
                {
                    Ok(user) => user,
                    Err(NotFound) => return Err(Error::UserDoesNotExist),
                    Err(_err) => return Err(Error::InternalError),
                }
            }
        };

        Ok(user.id)
    })
    .await
}

/**
 * Get a User by its Uuid
 */
pub async fn get_user(
    state: &web::Data<AppState>,
    uid: uuid::Uuid,
) -> Result<User, actix_web::Error> {
    use db_connector::schema::users::dsl::*;
    use diesel::prelude::*;

    let mut conn = get_connection(state)?;

    web_block_unpacked(move || {
        match users
            .find(uid)
            .select(User::as_select())
            .get_result(&mut conn)
        {
            Ok(u) => Ok(u),
            Err(NotFound) => Err(crate::error::Error::UserDoesNotExist),
            Err(_err) => Err(crate::error::Error::InternalError),
        }
    })
    .await
}

#[cfg(test)]
pub mod tests {
    use actix_web::{http::header::ContentType, test, App};
    use argon2::{password_hash::SaltString, Argon2, Params, PasswordHasher};
    use db_connector::{models::users::User, test_connection_pool};
    use diesel::prelude::*;
    use libsodium_sys::{
        crypto_box_SECRETKEYBYTES, crypto_secretbox_KEYBYTES, crypto_secretbox_MACBYTES,
        crypto_secretbox_NONCEBYTES, crypto_secretbox_easy,
    };
    use rand::{Rng, RngCore};
    use rand_core::OsRng;

    use crate::{
        models::response_auth_token::ResponseAuthorizationToken, routes::{
            auth::{
                get_login_salt::tests::get_test_login_salt,
                login::tests::login_user,
                register::{register, tests::delete_user, RegisterSchema},
                verify::tests::fast_verify,
            },
            charger::{
                add::tests::add_test_charger,
                allow_user::{tests::add_allowed_test_user, UserAuth},
                remove::tests::{remove_allowed_test_users, remove_test_charger, remove_test_keys},
                tests::TestCharger,
            },
        }, tests::configure
    };

    use super::create_authorization_token::tests::create_test_auth_token;

    // Get the uuid for an test user.
    pub fn get_test_uuid(mail: &str) -> Result<uuid::Uuid, anyhow::Error> {
        use db_connector::schema::users::dsl::*;

        let pool = test_connection_pool();
        let mut conn = pool.get().unwrap();
        let user: User = users
            .filter(email.eq(mail))
            .select(User::as_select())
            .get_result(&mut conn)?;

        Ok(user.id)
    }

    /**
    * Struct for testing with users.
      When using multiple instances create them in the opposite order they need to be destroyed in.
    */
    #[derive(Debug)]
    pub struct TestUser {
        pub mail: String,
        pub charger: Vec<TestCharger>,
        pub password: Vec<u8>,
        pub access_token: Option<String>,
        pub refresh_token: Option<String>,
    }

    pub fn hash_test_key(password: &[u8], salt: &[u8], len: Option<usize>) -> Vec<u8> {
        let len = len.unwrap_or(24);
        let params = Params::new(19 * 1024, 2, 1, Some(len)).unwrap();
        let argon2 = Argon2::default();
        let salt = SaltString::encode_b64(salt).unwrap();
        let hash = argon2
            .hash_password_customized(password, None, None, params, &salt)
            .unwrap();

        hash.hash.unwrap().as_bytes().to_owned()
    }

    pub fn generate_random_bytes_len(len: usize) -> Vec<u8> {
        let mut rng = rand::thread_rng();
        (0..len).map(|_| rng.gen_range(0..255)).collect()
    }

    impl TestUser {
        pub async fn new(mail: &str, secret: Option<Vec<u8>>) -> Self {
            let login_salt = generate_random_bytes_len(48);
            let secret_salt = generate_random_bytes_len(48);
            let password = generate_random_bytes_len(48);
            let secret = secret.unwrap_or(generate_random_bytes_len(
                crypto_box_SECRETKEYBYTES as usize,
            ));
            let login_key = hash_test_key(&password, &login_salt, None);
            let secret_key = hash_test_key(
                &password,
                &secret_salt,
                Some(crypto_secretbox_KEYBYTES as usize),
            );
            let secret_nonce = generate_random_bytes_len(crypto_secretbox_NONCEBYTES as usize);
            let mut encrypted_secret =
                vec![0u8; (crypto_secretbox_MACBYTES + crypto_box_SECRETKEYBYTES) as usize];
            unsafe {
                if crypto_secretbox_easy(
                    encrypted_secret.as_mut_ptr(),
                    secret.as_ptr(),
                    crypto_box_SECRETKEYBYTES as u64,
                    secret_nonce.as_ptr(),
                    secret_key.as_ptr(),
                ) != 0
                {
                    panic!("Encrypting secret failed.");
                }
            };

            let app = App::new().configure(configure).service(register);
            let app = test::init_service(app).await;
            let user = RegisterSchema {
                name: mail.to_string(),
                email: mail.to_string(),
                login_key,
                login_salt,
                secret: encrypted_secret,
                secret_nonce,
                secret_salt,
            };
            let req = test::TestRequest::post()
                .uri("/register")
                .insert_header(ContentType::json())
                .set_json(user)
                .to_request();
            let resp = test::call_service(&app, req).await;
            println!("{}", resp.status());
            assert!(resp.status().is_success());
            println!("Created user");

            fast_verify(mail);
            TestUser {
                mail: mail.to_string(),
                password,
                charger: Vec::new(),
                access_token: None,
                refresh_token: None,
            }
        }

        pub async fn random() -> (Self, String) {
            let uuid = uuid::Uuid::new_v4().to_string();
            let mail = format!("{}@test.invalid", uuid);
            let user = Self::new(&mail, None).await;
            (user, mail)
        }

        pub async fn random_with_secret(secret: Vec<u8>) -> (Self, String) {
            let uuid = uuid::Uuid::new_v4().to_string();
            let mail = format!("{}@test.invalid", uuid);
            let user = Self::new(&mail, Some(secret)).await;
            (user, mail)
        }

        pub fn get_access_token(&self) -> &str {
            self.access_token.as_ref().unwrap()
        }

        pub async fn login(&mut self) -> &str {
            if self.access_token.is_some() {
                return self.access_token.as_ref().unwrap();
            }
            let login_salt = get_test_login_salt(&self.mail).await;
            let login_key = hash_test_key(&self.password, &login_salt, None);

            let (access_token, refresh_token) = login_user(&self.mail, login_key).await;

            self.access_token = Some(access_token);
            self.refresh_token = Some(refresh_token);

            self.access_token.as_ref().unwrap()
        }

        pub async fn additional_login(&mut self) {
            let login_salt = get_test_login_salt(&self.mail).await;
            let login_key = hash_test_key(&self.password, &login_salt, None);
            login_user(&self.mail, login_key).await;
        }

        pub async fn get_login_key(&self) -> Vec<u8> {
            let login_salt = get_test_login_salt(&self.mail).await;
            let login_key = hash_test_key(&self.password, &login_salt, None);
            login_key
        }

        pub async fn add_charger(&mut self, id: i32) -> TestCharger {
            let charger = add_test_charger(id, self.access_token.as_ref().unwrap()).await;
            self.charger.push(charger.clone());

            charger
        }

        pub async fn add_random_charger(&mut self) -> TestCharger {
            let id = OsRng.next_u32() as i32;
            let charger = self.add_charger(id).await;

            charger
        }

        pub async fn allow_user(
            &mut self,
            email: &str,
            user_auth: UserAuth,
            charger: &TestCharger,
        ) {
            add_allowed_test_user(email, user_auth, charger).await;
        }

        pub fn get_mail(&self) -> &str {
            &self.mail
        }

        pub fn get_refresh_token(&mut self) -> &str {
            self.refresh_token.as_ref().unwrap()
        }

        pub async fn create_authorization_token(&self, use_once: bool) -> ResponseAuthorizationToken {
            create_test_auth_token(self, use_once).await
        }
    }

    impl Drop for TestUser {
        fn drop(&mut self) {
            while let Some(charger) = self.charger.pop() {
                let _ = remove_test_keys(&self.mail);
                remove_allowed_test_users(&charger.uuid);
                remove_test_charger(&charger.uuid);
            }
            delete_user(&self.mail);
        }
    }
}
