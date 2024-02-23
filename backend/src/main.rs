
pub use backend::*;

use actix_web::{middleware::Logger, web, App, HttpServer};
use db_connector::{get_connection_pool, run_migrations};
use lettre::{transport::smtp::authentication::Credentials, SmtpTransport};
use simplelog::{ColorChoice, CombinedLogger, Config, LevelFilter, TermLogger, TerminalMode};

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    CombinedLogger::init(vec![TermLogger::new(
        LevelFilter::Debug,
        Config::default(),
        TerminalMode::Mixed,
        ColorChoice::Auto,
    )])
    .unwrap();

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
        App::new()
            .wrap(cors)
            .wrap(Logger::default())
            .app_data(state.clone())
            .configure(routes::configure)
    })
    .bind("0.0.0.0:8081")?
    .run()
    .await
}
