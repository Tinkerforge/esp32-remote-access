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

use std::str::FromStr;

use actix_web::web;
use db_connector::models::chargers::Charger;
use diesel::prelude::*;
use diesel::{
    r2d2::{ConnectionManager, PooledConnection},
    result::Error::NotFound,
    PgConnection,
};
use rand::Rng;

use crate::{error::Error, routes::charger::add::password_matches, AppState};

pub fn get_connection(
    state: &web::Data<AppState>,
) -> actix_web::Result<PooledConnection<ConnectionManager<PgConnection>>> {
    match state.pool.get() {
        Ok(conn) => Ok(conn),
        Err(_err) => Err(Error::InternalError.into()),
    }
}

pub fn generate_random_bytes() -> Vec<u8> {
    let mut rng = rand::thread_rng();
    (0..24).map(|_| rng.gen_range(0..255)).collect()
}

pub async fn web_block_unpacked<F, R>(f: F) -> Result<R, actix_web::Error>
where
    F: FnOnce() -> Result<R, Error> + Send + 'static,
    R: Send + 'static,
{
    match web::block(f).await {
        Ok(res) => match res {
            Ok(v) => Ok(v),
            Err(err) => Err(err.into()),
        },
        Err(_err) => Err(Error::InternalError.into()),
    }
}

pub fn as_u8_slice<T: Sized>(p: &T) -> &[u8] {
    unsafe {
        ::core::slice::from_raw_parts((p as *const T) as *const u8, ::core::mem::size_of::<T>())
    }
}

pub fn parse_uuid(uuid: &str) -> actix_web::Result<uuid::Uuid> {
    match uuid::Uuid::from_str(uuid) {
        Ok(v) => Ok(v),
        Err(err) => Err(actix_web::error::ErrorBadRequest(err)),
    }
}

pub async fn get_charger_by_uid(
    uid: i32,
    password: Option<String>,
    state: &web::Data<AppState>,
) -> actix_web::Result<Charger> {
    let password = if let Some(password) = password {
        password
    } else {
        return Err(actix_web::error::ErrorBadRequest("Password is missing"));
    };

    let mut conn = get_connection(state)?;
    let chargers: Vec<Charger> = web_block_unpacked(move || {
        use db_connector::schema::chargers::dsl as chargers;

        match chargers::chargers
            .filter(chargers::uid.eq(uid))
            .select(Charger::as_select())
            .load(&mut conn)
        {
            Ok(c) => Ok(c),
            Err(NotFound) => {
                println!("C");
                Err(Error::ChargerCredentialsWrong)
            }
            Err(_err) => Err(Error::InternalError),
        }
    })
    .await?;

    for c in chargers.into_iter() {
        println!("D");
        if password_matches(&password, &c.password)? {
            return Ok(c);
        }
    }

    println!("E");
    Err(Error::ChargerCredentialsWrong.into())
}
