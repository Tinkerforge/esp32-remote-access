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
    cid: i32,
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
