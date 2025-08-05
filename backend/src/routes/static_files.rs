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

use actix_web::{web, HttpRequest, Result};
use actix_files::NamedFile;
use std::path::PathBuf;

// Define static file serving path based on build configuration

#[cfg(not(debug_assertions))]
const STATIC_SERVE_FROM: &str = "/static/";

async fn serve_gzip_static(req: HttpRequest) -> Result<NamedFile> {
    let path: PathBuf = req.match_info().query("filename").parse().unwrap();

    #[cfg(debug_assertions)]
    let static_serve_from = {
        let env = std::env::var("WARP_CHARGER_GIT_URL").unwrap_or_else(|_| "warp-charger".to_string());
        format!("{}/firmwares/static_html/", env)
    };
    #[cfg(not(debug_assertions))]
    let static_serve_from = "/static/";

    let base_path = PathBuf::from(static_serve_from);
    let full_path = base_path.join(&path);

    let file = NamedFile::open(full_path)?;
    Ok(file)
}

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::resource("/static/{filename:.*}")
            .route(web::get().to(serve_gzip_static))
    );
}
