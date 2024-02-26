use std::net::Ipv4Addr;

use actix_web::{App, HttpServer};
pub use backend::*;
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
    #[derive(OpenApi)]
    #[openapi(
        paths(
            routes::auth::login::login,
            routes::auth::logout::logout,
            routes::auth::register::register,
            routes::auth::verify::verify,
            routes::charger::add::add,
            routes::charger::allow_user::allow_user,
            routes::charger::remove::remove,
            routes::user::me::me,
            routes::user::update_password::update_password,
            routes::user::update_user::update_user,
        ),
        components(schemas(
            routes::auth::login::LoginSchema,
            routes::auth::register::RegisterSchema,
            routes::charger::add::AddChargerSchema,
            routes::charger::add::ChargerSchema,
            routes::charger::add::Keys,
            routes::charger::allow_user::AllowUserSchema,
            routes::charger::remove::DeleteChargerSchema,
            models::filtered_user::FilteredUser,
            routes::user::update_password::PasswordUpdateSchema,
        )),
        modifiers(&JwtToken)
    )]
    struct ApiDoc;

    let openapi = ApiDoc::openapi();

    HttpServer::new(move || {
        App::new().service(SwaggerUi::new("/{_:.*}").url("/api-docs/openapi.json", openapi.clone()))
    })
    .bind((Ipv4Addr::UNSPECIFIED, 12345))
    .unwrap()
    .run()
    .await
    .unwrap();
}
