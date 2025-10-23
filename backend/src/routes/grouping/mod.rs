/* esp32-remote-access
 * Copyright (C) 2025 Frederic Henrichs <frederic@tinkerforge.com>
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

pub mod create_grouping;
pub mod delete_grouping;
pub mod add_device_to_grouping;
pub mod remove_device_from_grouping;
pub mod get_groupings;

#[cfg(test)]
pub(crate) mod test_helpers;

use crate::middleware::jwt::JwtMiddleware;
use actix_web::web;

pub fn configure(cfg: &mut web::ServiceConfig) {
    let scope = web::scope("/grouping")
        .wrap(JwtMiddleware)
        .service(create_grouping::create_grouping)
        .service(delete_grouping::delete_grouping)
        .service(add_device_to_grouping::add_device_to_grouping)
        .service(remove_device_from_grouping::remove_device_from_grouping)
        .service(get_groupings::get_groupings);
    cfg.service(scope);
}

