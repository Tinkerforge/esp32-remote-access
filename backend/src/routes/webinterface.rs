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

use actix_files::NamedFile;
use actix_web::{get, web, Result};
use std::path::PathBuf;
use utoipa::IntoParams;

use crate::utils::{get_charger_from_db, parse_uuid};

#[cfg(not(debug_assertions))]
const STATIC_SERVE_FROM: &str = "/static/";

fn get_file(filename: String) -> Result<NamedFile> {
    let path: PathBuf = filename.parse().unwrap();

    #[cfg(debug_assertions)]
    let static_serve_from = {
        let env =
            std::env::var("WARP_CHARGER_GIT_PATH").unwrap_or_else(|_| "warp-charger".to_string());
        format!("{env}/firmwares/static_html/")
    };
    #[cfg(not(debug_assertions))]
    let static_serve_from = "/static/";

    let base_path = PathBuf::from(static_serve_from);
    let full_path = base_path.join(&path);

    let file = NamedFile::open(full_path)?;
    Ok(file)
}

#[derive(serde::Deserialize, serde::Serialize, IntoParams)]
pub struct StaticFileQuery {
    #[param(example = "550e8400-e29b-41d4-a716-446655440000")]
    charger: String,
}

#[utoipa::path(
    params(
        StaticFileQuery
    ),
    responses(
        (status = 200, description = "Webinterface HTML file"),
        (status = 400, description = "Invalid UUID format"),
        (status = 401, description = "Unauthorized"),
        (status = 404, description = "File not found"),
        (status = 500, description = "Internal server error"),
    ),
    security(
        ("jwt" = [])
    )
)]
#[get("/webinterface")]
pub async fn get_webinterface(
    query: web::Query<StaticFileQuery>,
    user: crate::models::uuid::Uuid,
    state: web::Data<crate::AppState>,
) -> Result<NamedFile> {
    let charger = parse_uuid(&query.charger)?;
    let user: uuid::Uuid = user.into();

    if !crate::routes::charger::user_is_allowed(&state, user, charger).await? {
        return Err(actix_web::error::ErrorUnauthorized("Unauthorized"));
    }

    let charger = get_charger_from_db(charger, &state).await?;
    let firmware = charger.firmware_version.replace('+', "_");
    let firmware = firmware.replace('.', "_");
    let firmware = format!("{firmware}_index.html");
    get_file(firmware)
}
