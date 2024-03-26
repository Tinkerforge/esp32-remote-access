use std::{
    collections::HashMap,
    net::UdpSocket,
    sync::{Arc, Mutex},
};

pub use backend::*;

use actix_web::{middleware::Logger, web, App, HttpServer};
use db_connector::{get_connection_pool, run_migrations, Pool};
use diesel::prelude::*;
use lettre::{transport::smtp::authentication::Credentials, SmtpTransport};
use simplelog::{ColorChoice, CombinedLogger, Config, LevelFilter, TermLogger, TerminalMode};

fn reset_wg_keys(pool: &Pool) {
    use db_connector::schema::wg_keys::dsl::*;

    let mut conn = pool.get().unwrap();
    diesel::update(wg_keys)
        .set(in_use.eq(false))
        .execute(&mut conn)
        .unwrap();
}

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

    reset_wg_keys(&pool);

    let mail = std::env::var("MAIL_USER").expect("MAIL_USER must be set");
    let pass = std::env::var("MAIL_PASS").expect("MAIL_PASS must be set");
    let relay = std::env::var("MAIL_RELAY").expect("MAIL_RELAY must be set");
    let port: u16 = std::env::var("MAIL_RELAY_PORT")
        .expect("MAIL_RELAY_PORT must be set")
        .parse()
        .unwrap();
    let mailer = SmtpTransport::starttls_relay(&relay)
        .unwrap()
        .port(port)
        .credentials(Credentials::new(mail, pass))
        .build();

    let state = web::Data::new(AppState {
        pool: pool.clone(),
        jwt_secret: std::env::var("JWT_SECRET").expect("JWT_SECRET must be set!"),
        mailer,
        frontend_url: std::env::var("FRONTEND_URL").expect("FRONTEND_URL must be set!"),
    });

    let bridge_state = web::Data::new(BridgeState {
        pool,
        web_client_map: Mutex::new(HashMap::new()),
        undiscovered_clients: Mutex::new(HashMap::new()),
        charger_management_map: Arc::new(Mutex::new(HashMap::new())),
        charger_management_map_with_id: Arc::new(Mutex::new(HashMap::new())),
        port_discovery: Arc::new(Mutex::new(HashMap::new())),
        charger_remote_conn_map: Mutex::new(HashMap::new()),
        socket: UdpSocket::bind("0.0.0.0:51820").unwrap(),
    });

    udp_server::start_server(bridge_state.clone()).unwrap();

    HttpServer::new(move || {
        let cors = actix_cors::Cors::permissive();
        App::new()
            .wrap(cors)
            .wrap(Logger::default())
            .app_data(state.clone())
            .app_data(bridge_state.clone())
            .configure(routes::configure)
    })
    .bind("0.0.0.0:8081")?
    .run()
    .await
}
