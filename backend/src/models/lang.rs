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

use std::future::{ready, Ready};

#[derive(Clone, Debug)]
pub struct Lang(String);

impl From<Lang> for String {
    fn from(value: Lang) -> Self {
        value.0
    }
}

impl actix_web::FromRequest for Lang {
    type Error = actix_web::Error;
    type Future = Ready<Result<Self, Self::Error>>;

    fn from_request(req: &actix_web::HttpRequest, _: &mut actix_web::dev::Payload) -> Self::Future {
        for (name, value) in req.headers().iter() {
            if name == "X-Lang" {
                if let Ok(value) = value.to_str() {
                    return ready(Ok(Lang(value.to_string())));
                }
                break;
            }
        }

        ready(Ok(Lang(String::new())))
    }
}
