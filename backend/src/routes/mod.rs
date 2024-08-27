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

pub mod auth;
pub mod charger;
pub mod management;
pub mod selfdestruct;
pub mod state;
pub mod user;

use actix_web::web::{self, scope};

use crate::{middleware::jwt::JwtMiddleware, ws_udp_bridge};

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.configure(user::configure);
    cfg.configure(auth::configure);
    cfg.configure(charger::configure);

    cfg.service(management::management);
    cfg.service(selfdestruct::selfdestruct);
    let scope = scope("")
        .wrap(JwtMiddleware)
        .service(ws_udp_bridge::start_ws);
    cfg.service(scope);
}
