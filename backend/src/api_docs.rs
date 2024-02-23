use std::net::Ipv4Addr;

use actix_web::{App, HttpServer};
pub use backend::*;
use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;

#[actix_web::main]
async fn main() {

    #[derive(OpenApi)]
    #[openapi(
        paths(
            routes::auth::login::login,
            routes::auth::logout::logout,
            routes::auth::register::register,
            routes::auth::verify::verify,
        ),
        components(
            schemas(
                routes::auth::login::LoginSchema,
                routes::auth::register::RegisterSchema,
            )
        )
    )]
    struct ApiDoc;

    let openapi = ApiDoc::openapi();

    HttpServer::new(move || {
        App::new()
            .service(SwaggerUi::new("/{_:.*}").url("/api-docs/openapi.json", openapi.clone()))
    }).bind((Ipv4Addr::UNSPECIFIED, 12345)).unwrap().run().await.unwrap();
}
