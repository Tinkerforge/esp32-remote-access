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

use std::net::Ipv4Addr;

use actix_web::{middleware::Logger, App, HttpServer};
pub use backend::*;
use log::LevelFilter;
use simplelog::{ColorChoice, CombinedLogger, Config, TermLogger, TerminalMode};
use utoipa::{
    openapi::security::{HttpAuthScheme, HttpBuilder, SecurityScheme},
    Modify, OpenApi,
};
use utoipa_swagger_ui::SwaggerUi;

struct JwtToken;

impl Modify for JwtToken {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        let components = openapi.components.as_mut().unwrap();
        components.add_security_scheme(
            "jwt",
            SecurityScheme::Http(
                HttpBuilder::new()
                    .scheme(HttpAuthScheme::Bearer)
                    .bearer_format("JWT")
                    .build(),
            ),
        )
    }
}

/**
 * Start a server that hosts the api documentation.
 */
#[actix_web::main]
async fn main() {
    CombinedLogger::init(vec![TermLogger::new(
        LevelFilter::Debug,
        Config::default(),
        TerminalMode::Mixed,
        ColorChoice::Auto,
    )])
    .unwrap();
    #[derive(OpenApi)]
    #[openapi(
        paths(
            routes::auth::login::login,
            routes::auth::logout::logout,
            routes::auth::register::register,
            routes::auth::verify::verify,
            routes::auth::generate_salt::generate_salt,
            routes::auth::get_login_salt::get_login_salt,
            routes::charger::add::add,
            routes::charger::allow_user::allow_user,
            routes::charger::remove::remove,
            routes::charger::get_chargers::get_chargers,
            routes::charger::get_key::get_key,
            routes::user::me::me,
            routes::user::update_password::update_password,
            routes::user::update_user::update_user,
            routes::user::get_secret::get_secret,
            routes::management::management,
        ),
        components(schemas(
            routes::auth::login::LoginSchema,
            routes::auth::register::RegisterSchema,
            routes::charger::add::AddChargerSchema,
            routes::charger::add::ChargerSchema,
            routes::charger::add::Keys,
            routes::charger::add::AddChargerResponseSchema,
            routes::charger::allow_user::AllowUserSchema,
            routes::charger::remove::DeleteChargerSchema,
            routes::charger::get_chargers::GetChargerSchema,
            routes::charger::get_key::GetWgKeysSchema,
            routes::user::update_password::PasswordUpdateSchema,
            routes::user::get_secret::GetSecretResponse,
            routes::management::ManagementSchema,
            routes::management::ManagementResponseSchema,
            models::filtered_user::FilteredUser,
        )),
        modifiers(&JwtToken)
    )]
    struct ApiDoc;

    let openapi = ApiDoc::openapi();

    HttpServer::new(move || {
        App::new()
            .wrap(Logger::default())
            .service(SwaggerUi::new("/{_:.*}").url("/api-docs/openapi.json", openapi.clone()))
    })
    .bind((Ipv4Addr::UNSPECIFIED, 12345))
    .unwrap()
    .run()
    .await
    .unwrap();
}
