mod routes;
mod models;
mod middleware;
mod utils;
mod error;

use actix_web::{web, App, HttpServer};
use db_connector::*;
use lettre::{transport::smtp::authentication::Credentials, SmtpTransport};
use routes::register_routes;

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    dotenv::dotenv().ok();

    let pool = get_connection_pool();
    let mut conn = pool.get().expect("Failed to get connection from pool");
    run_migrations(&mut conn).expect("Failed to run migrations");

    let mail = std::env::var("MAIL_USER").expect("MAIL_USER must be set");
    let pass = std::env::var("MAIL_PASS").expect("MAIL_PASS must be set");
    let mailer = SmtpTransport::relay("mail.tinkerforge.com")
        .unwrap()
        .port(465)
        .credentials(Credentials::new(mail, pass))
        .build();

    let state = web::Data::new(AppState {
        pool,
        jwt_secret: std::env::var("JWT_SECRET").expect("JWT_SECRET must be set!"),
        mailer,
        frontend_url: std::env::var("FRONTEND_URL").expect("FRONTEND_URL must be set!"),
    });

    HttpServer::new(move || {
        let cors = actix_cors::Cors::permissive();
        let app = App::new()
            .wrap(cors)
            .app_data(state.clone());

        register_routes(app)
    })
    .bind("0.0.0.0:8081")?
    .run()
    .await
}

pub struct AppState {
    pub pool: Pool,
    pub jwt_secret: String,
    pub mailer: SmtpTransport,
    pub frontend_url: String,
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use actix_web::web::ServiceConfig;

    pub struct ScopeCall<F: FnMut()> {
        pub c: F
    }
    impl <F: FnMut()> Drop for ScopeCall<F> {
        fn drop(&mut self) {
            (self.c)();
        }
    }

    #[macro_export]
    macro_rules! defer {
        ($e:expr) => {
            let _scope_call = crate::tests::ScopeCall { c: || -> () { $e; }};
        };
    }

    pub fn configure(cfg: &mut ServiceConfig) {
        let pool = db_connector::test_connection_pool();

        let mail = std::env::var("MAIL_USER").expect("MAIL must be set");
        let pass = std::env::var("MAIL_PASS").expect("MAIL_PASS must be set");
        let mailer = SmtpTransport::relay("mail.tinkerforge.com")
            .unwrap()
            .port(465)
            .credentials(Credentials::new(mail, pass))
            .build();


        let state = AppState {
            pool,
            jwt_secret: std::env::var("JWT_SECRET").expect("JWT_SECRET must be set!"),
            mailer,
            frontend_url: std::env::var("FRONTEND_URL").expect("FRONTEND_URL must be set!"),
        };
        let state = web::Data::new(state);
        cfg.app_data(state.clone());
    }
}
