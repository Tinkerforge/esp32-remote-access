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

pub mod me;
pub mod update_password;
pub mod update_user;
pub mod generate_salt;

use crate::{
    error::Error,
    middleware::jwt::JwtMiddleware,
    utils::{get_connection, web_block_unpacked},
    AppState,
};
use actix_web::web::{self, ServiceConfig};
use db_connector::models::users::User;
use diesel::{prelude::*, result::Error::NotFound, ExpressionMethods};

pub fn configure(cfg: &mut ServiceConfig) {
    let scope = web::scope("/user")
        .wrap(JwtMiddleware)
        .service(update_user::update_user)
        .service(update_password::update_password)
        .service(generate_salt::generate_salt)
        .service(me::me);
    cfg.service(scope);
}

/**
 * Lookup the corresponding Uuid for an email.
 */
pub async fn get_uuid_from_email(
    state: &web::Data<AppState>,
    mail: String,
) -> Result<uuid::Uuid, actix_web::Error> {
    use db_connector::schema::users::dsl::*;

    let mut conn = get_connection(state)?;
    web_block_unpacked(move || {
        let user: User = match users
            .filter(email.eq(mail))
            .select(User::as_select())
            .get_result(&mut conn)
        {
            Ok(user) => user,
            Err(NotFound) => return Err(Error::UserDoesNotExist),
            Err(_err) => return Err(Error::InternalError),
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
    pub fn get_test_uuid(mail: &str) -> uuid::Uuid {
        use db_connector::schema::users::dsl::*;

        let pool = test_connection_pool();
        let mut conn = pool.get().unwrap();
        let user: User = users
            .filter(email.eq(mail))
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
        mail: String,
        charger: Vec<i32>,
        login_key: Vec<u8>,
        token: Option<String>,
    }

    impl TestUser {
        pub async fn new(mail: &str) -> Self {
            let key = create_user(mail).await;
            fast_verify(mail);
            TestUser {
                mail: mail.to_string(),
                login_key: key,
                charger: Vec::new(),
                token: None,
            }
        }

        pub async fn random() -> (Self, String) {
            let uuid = uuid::Uuid::new_v4().to_string();
            let mail = format!("{}@test.invalid", uuid);
            let user = Self::new(&mail).await;
            (user, mail)
        }

        pub fn get_token(&self) -> &str {
            self.token.as_ref().unwrap()
        }

        pub async fn login(&mut self) -> &str {
            if self.token.is_some() {
                return self.token.as_ref().unwrap();
            }
            self.token = Some(login_user(&self.mail, self.login_key.clone()).await);

            self.token.as_ref().unwrap()
        }

        pub async fn add_charger(&mut self, id: i32) {
            add_test_charger(id, self.token.as_ref().unwrap()).await;
            self.charger.push(id);
        }

        pub async fn add_random_charger(&mut self) -> i32 {
            let charger = OsRng.next_u32() as i32;
            self.add_charger(charger).await;

            charger
        }

        pub async fn allow_user(&mut self, user_mail: &str, charger_id: i32) {
            let token = self.token.as_ref().expect("Test user must be logged in.");
            add_allowed_test_user(user_mail, charger_id, token).await;
        }

        pub fn get_mail(&self) -> &str {
            &self.mail
        }
    }

    impl Drop for TestUser {
        fn drop(&mut self) {
            while let Some(charger) = self.charger.pop() {
                remove_test_keys(&self.mail);
                remove_allowed_test_users(charger);
                remove_test_charger(charger);
            }
            delete_user(&self.mail);
        }
    }
}
