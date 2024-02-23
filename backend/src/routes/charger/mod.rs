pub(crate) mod add;
mod allow_user;
pub(crate) mod remove;

use crate::{
    error::Error,
    middleware::jwt::JwtMiddleware,
    utils::{get_connection, web_block_unpacked},
    AppState,
};
use actix_web::web;
use db_connector::models::allowed_users::AllowedUser;
use diesel::prelude::*;

pub fn configure(cfg: &mut web::ServiceConfig) {
    let scope = web::scope("/wallbox")
        .wrap(JwtMiddleware)
        .service(add::add)
        .service(remove::remove);
    cfg.service(scope);
}

pub async fn charger_belongs_to_user(
    state: &web::Data<AppState>,
    uid: uuid::Uuid,
    cid: String,
) -> Result<bool, actix_web::Error> {
    use crate::schema::allowed_users::dsl::*;

    let mut conn = get_connection(state)?;
    let owner = web_block_unpacked(move || {
        let allowed_user: AllowedUser = match allowed_users
            .filter(user.eq(uid))
            .filter(charger.eq(cid))
            .select(AllowedUser::as_select())
            .get_result(&mut conn)
        {
            Ok(u) => u,
            Err(_err) => return Err(Error::InternalError),
        };

        Ok(allowed_user.is_owner)
    })
    .await?;

    Ok(owner)
}
