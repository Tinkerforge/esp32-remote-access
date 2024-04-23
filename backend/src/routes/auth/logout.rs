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

use actix_web::{
    cookie::{time::Duration, Cookie}, get, HttpRequest, HttpResponse, Responder
};

use crate::middleware::get_token;

/// Logout user
#[utoipa::path(
    context_path = "/auth",
    responses(
        (status = 200, description = "User logged out")
    )
)]
#[get("/logout")]
pub async fn logout(req: HttpRequest) -> impl Responder {
    if let Some(token) = get_token(&req, "request_token") {
        
    }

    let cookie = Cookie::build("access_token", "")
        .path("/")
        .max_age(Duration::new(-1, 0))
        .http_only(true)
        .finish();

    HttpResponse::Ok().cookie(cookie).body("")
}
