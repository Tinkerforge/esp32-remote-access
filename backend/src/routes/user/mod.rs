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

pub mod get_secret;
pub mod logout;
pub mod me;
pub mod update_password;
pub mod update_user;

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
        .service(me::me);
    cfg.service(scope);
}

/**
 * Lookup the corresponding Uuid for an email or username or check if uuid exists.
 */
pub async fn get_uuid(
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
    use db_connector::{models::users::User, test_connection_pool};
    use diesel::prelude::*;
    use rand::RngCore;
    use rand_core::OsRng;

    use crate::routes::{
        auth::{
            login::tests::login_user,
            register::tests::{create_user, delete_user},
            verify::tests::fast_verify,
        },
        charger::{
            add::tests::add_test_charger,
            allow_user::tests::add_allowed_test_user,
            remove::tests::{remove_allowed_test_users, remove_test_charger, remove_test_keys},
        },
    };

    // Get the uuid for an test user.
    pub fn get_test_uuid(username: &str) -> uuid::Uuid {
        use db_connector::schema::users::dsl::*;

        let pool = test_connection_pool();
        let mut conn = pool.get().unwrap();
        let user: User = users
            .filter(name.eq(username))
            .select(User::as_select())
            .get_result(&mut conn)
            .unwrap();

        user.id
    }

    /**
    * Struct for testing with users.
      When using multiple instances create them in the opposite order they need to destroyed in.
    */
    #[derive(Debug)]
    pub struct TestUser {
        username: String,
        mail: String,
        charger: Vec<i32>,
        login_key: Vec<u8>,
        access_token: Option<String>,
        refresh_token: Option<String>,
    }

    impl TestUser {
        pub async fn new(mail: &str, username: &str) -> Self {
            let key = create_user(mail, username).await;
            fast_verify(mail);
            TestUser {
                username: username.to_string(),
                mail: mail.to_string(),
                login_key: key,
                charger: Vec::new(),
                access_token: None,
                refresh_token: None,
            }
        }

        pub async fn random() -> (Self, String) {
            let uuid = uuid::Uuid::new_v4().to_string();
            let mail = format!("{}@test.invalid", uuid);
            let user = Self::new(&mail, &uuid.to_string()).await;
            (user, uuid.to_string())
        }

        pub fn get_access_token(&self) -> &str {
            self.access_token.as_ref().unwrap()
        }

        pub async fn login(&mut self) -> &str {
            if self.access_token.is_some() {
                return self.access_token.as_ref().unwrap();
            }
            let (access_token, refresh_token) =
                login_user(&self.mail, self.login_key.clone()).await;

            self.access_token = Some(access_token);
            self.refresh_token = Some(refresh_token);

            self.access_token.as_ref().unwrap()
        }

        pub async fn additional_login(&mut self) {
            login_user(&self.mail, self.login_key.clone()).await;
        }

        pub async fn add_charger(&mut self, id: i32) -> String {
            let pass = add_test_charger(id, self.access_token.as_ref().unwrap()).await;
            self.charger.push(id);

            pass
        }

        pub async fn add_random_charger(&mut self) -> (i32, String) {
            let charger = OsRng.next_u32() as i32;
            let pass = self.add_charger(charger).await;

            (charger, pass)
        }

        pub async fn allow_user(&mut self, email: &str, charger_id: i32) {
            let token = self
                .access_token
                .as_ref()
                .expect("Test user must be logged in.");
            add_allowed_test_user(email, charger_id, token).await;
        }

        pub fn get_mail(&self) -> &str {
            &self.mail
        }

        pub fn get_refresh_token(&mut self) -> &str {
            self.refresh_token.as_ref().unwrap()
        }
    }

    impl Drop for TestUser {
        fn drop(&mut self) {
            while let Some(charger) = self.charger.pop() {
                remove_test_keys(&self.username);
                remove_allowed_test_users(charger);
                remove_test_charger(charger);
            }
            delete_user(&self.mail);
        }
    }
}
