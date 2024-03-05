pub mod add;
pub mod allow_user;
pub mod get_chargers;
pub mod get_key;
pub mod remove;

use crate::{
    error::Error,
    middleware::jwt::JwtMiddleware,
    utils::{get_connection, web_block_unpacked},
    AppState,
};
use actix_web::web;
use db_connector::models::allowed_users::AllowedUser;
use diesel::{prelude::*, result::Error::NotFound};

pub fn configure(cfg: &mut web::ServiceConfig) {
    let scope = web::scope("/charger")
        .wrap(JwtMiddleware)
        .service(add::add)
        .service(allow_user::allow_user)
        .service(remove::remove)
        .service(get_chargers::get_chargers)
        .service(get_key::get_key);
    cfg.service(scope);
}

pub async fn charger_belongs_to_user(
    state: &web::Data<AppState>,
    uid: uuid::Uuid,
    cid: String,
) -> Result<bool, actix_web::Error> {
    use db_connector::schema::allowed_users::dsl::*;

    let mut conn = get_connection(state)?;
    let owner = web_block_unpacked(move || {
        let allowed_user: AllowedUser = match allowed_users
            .filter(user_id.eq(uid))
            .filter(charger_id.eq(cid))
            .select(AllowedUser::as_select())
            .get_result(&mut conn)
        {
            Ok(u) => u,
            Err(NotFound) => return Err(Error::UserIsNotOwner),
            Err(_err) => return Err(Error::InternalError),
        };

        Ok(allowed_user.is_owner)
    })
    .await?;

    Ok(owner)
}
