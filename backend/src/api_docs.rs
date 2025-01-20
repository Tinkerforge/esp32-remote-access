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
struct RefreshToken;

impl Modify for RefreshToken {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        let components = openapi.components.as_mut().unwrap();
        components.add_security_scheme(
            "refresh",
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
            routes::auth::register::register,
            routes::auth::verify::verify,
            routes::auth::generate_salt::generate_salt,
            routes::auth:: jwt_refresh::jwt_refresh,
            routes::auth::get_login_salt::get_login_salt,
            routes::auth::recovery::recovery,
            routes::auth::start_recovery::start_recovery,
            routes::charger::add::add,
            routes::charger::allow_user::allow_user,
            routes::charger::remove::remove,
            routes::charger::get_chargers::get_chargers,
            routes::charger::get_key::get_key,
            routes::charger::update_note::update_note,
            routes::charger::add_with_token::add_with_token,
            routes::selfdestruct::selfdestruct,
            routes::user::me::me,
            routes::user::logout::logout,
            routes::user::update_password::update_password,
            routes::user::update_user::update_user,
            routes::user::get_secret::get_secret,
            routes::user::create_authorization_token::create_authorization_token,
            routes::user::get_authorization_tokens::get_authorization_tokens,
            routes::user::delete_authorization_token::delete_authorization_token,
            routes::user::delete::delete_user,
            routes::management::management,
        ),
        components(schemas(
            routes::auth::login::LoginSchema,
            routes::auth::register::RegisterSchema,
            routes::auth::recovery::RecoverySchema,
            routes::charger::add::AddChargerSchema,
            routes::charger::add::ChargerSchema,
            routes::charger::add::Keys,
            routes::charger::add::AddChargerResponseSchema,
            routes::charger::allow_user::UserAuth,
            routes::charger::allow_user::AllowUserSchema,
            routes::charger::remove::DeleteChargerSchema,
            routes::charger::get_chargers::ChargerStatus,
            routes::charger::get_chargers::GetChargerSchema,
            routes::charger::update_note::UpdateNoteSchema,
            routes::selfdestruct::SelfdestructSchema,
            routes::charger::get_key::GetWgKeysResponseSchema,
            routes::charger::add_with_token::AddChargerWithTokenSchema,
            routes::user::update_password::PasswordUpdateSchema,
            routes::user::get_secret::GetSecretResponse,
            routes::user::delete::DeleteUserSchema,
            routes::user::create_authorization_token::CreateAuthorizationTokenSchema,
            routes::user::get_authorization_tokens::GetAuthorizationTokensResponseSchema,
            routes::user::delete_authorization_token::DeleteAuthorizationTokenSchema,
            routes::user::update_user::UpdateUserSchema,
            routes::user::me::UserInfo,
            routes::management::ManagementSchema,
            routes::management::ManagementResponseSchema,
            routes::management::ManagementDataVersion,
            routes::management::ManagementDataVersion1,
            routes::management::ManagementDataVersion2,
            routes::management::ConfiguredUser,
            models::response_auth_token::ResponseAuthorizationToken,
        )),
        modifiers(&JwtToken, &RefreshToken)
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
