mod me;
mod update_password;
mod update_user;

use crate::{middleware::jwt::JwtMiddleware, utils::get_connection, AppState};
use actix_web::web::{self, ServiceConfig};
use db_connector::models::users::User;
use diesel::result::Error::NotFound;

pub fn configure(cfg: &mut ServiceConfig) {
    let scope = web::scope("/user")
        .wrap(JwtMiddleware)
        .service(update_user::update_user)
        .service(update_password::update_password)
        .service(me::me);
    cfg.service(scope);
}

pub async fn get_user(
    state: &web::Data<AppState>,
    uid: uuid::Uuid,
) -> Result<User, actix_web::Error> {
    use crate::schema::users::dsl::*;
    use diesel::prelude::*;

    let mut conn = get_connection(state)?;

    match web::block(move || {
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
    {
        Ok(res) => match res {
            Ok(u) => Ok(u),
            Err(err) => Err(err.into()),
        },
        Err(_err) => Err(crate::error::Error::InternalError.into()),
    }
}

#[cfg(test)]
pub mod tests {
    use db_connector::{models::users::User, test_connection_pool};
    use diesel::prelude::*;

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
}
