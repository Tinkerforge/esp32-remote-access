mod me;
mod update_password;
mod update_user;

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
    use crate::schema::users::dsl::*;

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
    use crate::schema::users::dsl::*;
    use diesel::prelude::*;

    let mut conn = get_connection(state)?;

    web_block_unpacked(move || {
        match users
            .find(uid)
            .select(User::as_select())
            .get_result(&mut conn)
        {
            Ok(u) => Ok(u),
            Err(NotFound) => Err(crate::error::Error::InternalError),
            Err(_err) => Err(crate::error::Error::InternalError),
        }
    })
    .await
}

#[cfg(test)]
pub mod tests {
    use db_connector::{models::users::User, test_connection_pool};
    use diesel::prelude::*;

    use crate::routes::{auth::{login::tests::login_user, register::tests::{create_user, delete_user}, verify::tests::fast_verify}, charger::add::tests::add_test_charger};

    // Get the uuid for an test user.
    pub fn get_test_uuid(mail: &str) -> uuid::Uuid {
        use crate::schema::users::dsl::*;

        let pool = test_connection_pool();
        let mut conn = pool.get().unwrap();
        let user: User = users
            .filter(email.eq(mail))
            .select(User::as_select())
            .get_result(&mut conn)
            .unwrap();

        user.id
    }

    pub struct TestUser {
        mail: String,
        charger: Vec<String>,
        token: Option<String>,
    }

    impl TestUser {
        pub async fn new(mail: &str) -> Self {
            create_user(mail).await;
            fast_verify(mail);
            TestUser {
                mail: mail.to_string(),
                charger: Vec::new(),
                token: None
            }
        }

        pub async fn login(&mut self) -> &str {
            if self.token.is_some() {
                return self.token.as_ref().unwrap()
            }
            self.token = Some(login_user(&self.mail, None).await);
            self.token.as_ref().unwrap()
        }

        pub async fn add_charger(&mut self, name: &str) {
            add_test_charger(name, self.token.as_ref().unwrap()).await;
            self.charger.push(name.to_string());
        }
    }

    impl Drop for TestUser {
        fn drop(&mut self) {
            delete_user(&self.mail);
        }
    }
}
