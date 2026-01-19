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

mod monitoring;

use std::{
    collections::{HashMap, HashSet},
    io::BufReader,
    net::UdpSocket,
    num::NonZeroUsize,
    sync::Arc,
    time::Duration,
};

use actix::Arbiter;
use actix_files::{Files, NamedFile};
use actix_web::{
    dev::{fn_service, ServiceRequest, ServiceResponse},
    middleware::{Compress, Logger},
    web, App, HttpServer,
};
pub use backend::*;
use backend::{rate_limit::IPRateLimiter, utils::get_connection};

use db_connector::{get_connection_pool, run_migrations};
use futures_util::lock::Mutex;
use lettre::{transport::smtp::authentication::Credentials, SmtpTransport};
use lru::LruCache;
use rate_limit::{ChargerRateLimiter, LoginRateLimiter};
use rustls::ServerConfig;
use simplelog::{
    ColorChoice, CombinedLogger, ConfigBuilder, LevelFilter, TermLogger, TerminalMode,
};

#[cfg(not(debug_assertions))]
use simplelog::WriteLogger;

use udp_server::packet::{
    ManagementCommand, ManagementCommandId, ManagementCommandPacket, ManagementPacket,
    ManagementPacketHeader,
};

fn cleanup_thread(state: web::Data<AppState>) {
    loop {
        std::thread::sleep(Duration::from_secs(60));

        let mut conn = match get_connection(&state) {
            Ok(c) => c,
            Err(_err) => {
                continue;
            }
        };

        clean_refresh_tokens(&mut conn);
        clean_recovery_tokens(&mut conn);
        clean_verification_tokens(&mut conn);
        clean_chargers(&mut conn);
    }
}

async fn resend_thread(bridge_state: web::Data<BridgeState>) {
    loop {
        std::thread::sleep(Duration::from_secs(1));
        let undiscovered_ports = bridge_state.port_discovery.lock().await;
        for (port, _) in undiscovered_ports.iter() {
            let command = ManagementCommand {
                command_id: ManagementCommandId::Connect,
                connection_no: port.connection_no,
                connection_uuid: port.connection_uuid,
            };

            let header = ManagementPacketHeader {
                magic: 0x1234,
                length: std::mem::size_of::<ManagementCommand>() as u16,
                seq_number: 0,
                version: 1,
                p_type: 0x00,
            };

            let packet = ManagementCommandPacket { header, command };
            let charger_id = uuid::Uuid::from_u128(port.charger_id);
            let chargers = bridge_state.charger_management_map_with_id.lock().await;
            if let Some(sock) = chargers.get(&charger_id) {
                let mut sock = sock.lock().await;
                sock.send_packet(ManagementPacket::CommandPacket(packet));
            }
        }
    }
}

fn load_rustls_config() -> Option<ServerConfig> {
    let cert_path = std::env::var("TLS_CERT_PATH").ok()?;
    let key_path = std::env::var("TLS_KEY_PATH").ok()?;

    let cert_file = &mut BufReader::new(std::fs::File::open(&cert_path).ok()?);
    let key_file = &mut BufReader::new(std::fs::File::open(&key_path).ok()?);

    let cert_chain: Vec<_> = rustls_pemfile::certs(cert_file)
        .filter_map(|c| c.ok())
        .collect();
    let key = rustls_pemfile::private_key(key_file).ok()??;

    let config = ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(cert_chain, key)
        .ok()?;

    Some(config)
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    let log_config = ConfigBuilder::new()
        .set_time_format_rfc3339()
        .set_time_offset_to_local()
        .unwrap()
        .build();

    #[cfg(not(debug_assertions))]
    let write_logger = WriteLogger::new(
        LevelFilter::Info,
        log_config.clone(),
        std::fs::File::create(format!("/logs/backend-{}.log", chrono::Local::now().format("%Y-%m-%d-%H"))).unwrap(),
    );

    #[cfg(debug_assertions)]
    CombinedLogger::init(vec![TermLogger::new(
        LevelFilter::Debug,
        log_config,
        TerminalMode::Mixed,
        ColorChoice::Auto,
    )])
    .unwrap();

    #[cfg(not(debug_assertions))]
    CombinedLogger::init(vec![
        TermLogger::new(
            LevelFilter::Info,
            log_config,
            TerminalMode::Mixed,
            ColorChoice::Auto,
        ),
        write_logger,
    ])
    .unwrap();

    dotenvy::dotenv().ok();

    let pool = get_connection_pool();
    let mut conn = pool.get().expect("Failed to get connection from pool");
    run_migrations(&mut conn).expect("Failed to run migrations");

    let mailer = {
        let email = std::env::var("EMAIL_USER").expect("EMAIL_USER must be set");
        let pass = std::env::var("EMAIL_PASS").expect("EMAIL_PASS must be set");
        let relay = std::env::var("EMAIL_RELAY").expect("EMAIL_RELAY must be set");
        let port: u16 = std::env::var("EMAIL_RELAY_PORT")
            .expect("EMAIL_RELAY_PORT must be set")
            .parse()
            .unwrap();
        SmtpTransport::relay(&relay)
            .unwrap()
            .port(port)
            .credentials(Credentials::new(email, pass))
            .build()
    };

    let sender_email = std::env::var("SENDER_EMAIL").expect("SENDER_EMAIL must be set");
    let sender_name = std::env::var("SENDER_NAME").expect("SENDER_NAME must be set");
    let brand = backend::branding::Brand::from_env();

    let state = web::Data::new(AppState {
        pool: pool.clone(),
        jwt_secret: std::env::var("JWT_SECRET").expect("JWT_SECRET must be set!"),
        mailer: Some(mailer),
        frontend_url: std::env::var("FRONTEND_URL").expect("FRONTEND_URL must be set!"),
        sender_email,
        sender_name,
        brand,
        keys_in_use: Mutex::new(HashSet::new()),
        hasher: backend::hasher::HasherManager::default(),
    });

    monitoring::start_monitoring(state.clone());

    let bridge_state = web::Data::new(BridgeState {
        pool,
        web_client_map: Mutex::new(HashMap::new()),
        undiscovered_clients: Mutex::new(HashMap::new()),
        charger_management_map: Arc::new(Mutex::new(HashMap::new())),
        charger_management_map_with_id: Arc::new(Mutex::new(HashMap::new())),
        port_discovery: Arc::new(Mutex::new(HashMap::new())),
        charger_remote_conn_map: Mutex::new(HashMap::new()),
        undiscovered_chargers: Arc::new(Mutex::new(HashMap::new())),
        lost_connections: Mutex::new(HashMap::new()),
        socket: Arc::new(UdpSocket::bind("0.0.0.0:51820").unwrap()),
        state_update_clients: Mutex::new(HashMap::new()),
    });

    let state_cpy = state.clone();
    std::thread::spawn(move || cleanup_thread(state_cpy));
    let bridge_state_cpy = bridge_state.clone();
    let arbiter = Arbiter::new();
    arbiter.spawn(async move { resend_thread(bridge_state_cpy).await });

    udp_server::start_server(bridge_state.clone(), state.clone());

    // Cache for random salts of non existing users
    let cache: web::Data<std::sync::Mutex<LruCache<String, Vec<u8>>>> = web::Data::new(
        std::sync::Mutex::new(LruCache::new(NonZeroUsize::new(10000).unwrap())),
    );

    let login_ratelimiter = web::Data::new(LoginRateLimiter::new());
    let charger_ratelimiter = web::Data::new(ChargerRateLimiter::new());
    let general_ratelimiter = web::Data::new(IPRateLimiter::new());

    let static_files_dir = std::env::var("STATIC_FILES_DIR").unwrap();

    let server = HttpServer::new(move || {
        let cors = actix_cors::Cors::permissive();
        let static_dir = static_files_dir.clone();
        App::new()
            .wrap(cors)
            .wrap(Compress::default())
            .wrap(Logger::default())
            .app_data(cache.clone())
            .app_data(state.clone())
            .app_data(login_ratelimiter.clone())
            .app_data(charger_ratelimiter.clone())
            .app_data(general_ratelimiter.clone())
            .app_data(bridge_state.clone())
            .service(web::scope("/api").configure(routes::configure))
            .service(
                Files::new("/", &static_dir)
                    .index_file("index.html")
                    .default_handler(fn_service(move |req: ServiceRequest| {
                        let static_dir = static_dir.clone();
                        async move {
                            let (req, _) = req.into_parts();
                            let index_path = format!("{}/index.html", static_dir);
                            let file = NamedFile::open(&index_path)?.set_content_disposition(
                                actix_web::http::header::ContentDisposition {
                                    disposition: actix_web::http::header::DispositionType::Inline,
                                    parameters: vec![],
                                },
                            );
                            let res = file.into_response(&req);
                            Ok(ServiceResponse::new(req, res))
                        }
                    })),
            )
    });

    #[cfg(debug_assertions)]
    let port = "8081";
    #[cfg(not(debug_assertions))]
    let port = "443";

    let addr = format!("0.0.0.0:{port}");

    // crash in case of TLS loading error
    let tls_config = load_rustls_config().unwrap();
    server.bind_rustls_0_23(&addr, tls_config)?.run().await?;

    Ok(())
}
