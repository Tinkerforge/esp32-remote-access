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

use actix_web::web::{self, ServiceConfig};

pub mod generate_salt;
pub mod get_login_salt;
pub mod jwt_refresh;
pub mod login;
pub mod recovery;
pub mod register;
pub mod start_recovery;
pub mod verify;
pub mod resend_verification;

pub const VERIFICATION_EXPIRATION_DAYS: u64 = 1;

pub fn configure(cfg: &mut ServiceConfig) {
    let scope = web::scope("/auth")
        .service(register::register)
        .service(resend_verification::resend_verification)
        .service(verify::verify)
        .service(get_login_salt::get_login_salt)
        .service(generate_salt::generate_salt)
        .service(jwt_refresh::jwt_refresh)
        .service(start_recovery::start_recovery)
        .service(recovery::recovery)
        .service(login::login);
    cfg.service(scope);
}
