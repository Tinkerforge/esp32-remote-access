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

use std::{
    collections::HashMap,
    net::UdpSocket,
    sync::{Arc, Mutex},
    time::Duration,
};

use backend::utils::get_connection;
pub use backend::*;

use actix_web::{middleware::Logger, web, App, HttpServer};
use chrono::Utc;
use db_connector::{get_connection_pool, run_migrations, Pool};
use diesel::prelude::*;
use lettre::{transport::smtp::authentication::Credentials, SmtpTransport};
use simplelog::{ColorChoice, CombinedLogger, Config, LevelFilter, TermLogger, TerminalMode};
use udp_server::packet::{ManagementCommand, ManagementCommandId, ManagementCommandPacket, ManagementPacket, ManagementPacketHeader};

fn reset_wg_keys(pool: &Pool) {
    use db_connector::schema::wg_keys::dsl::*;

    let mut conn = pool.get().unwrap();
    diesel::update(wg_keys)
        .set(in_use.eq(false))
        .execute(&mut conn)
        .unwrap();
}

fn cleanup_thread(state: web::Data<AppState>) {
    loop {
        use db_connector::schema::refresh_tokens::dsl::*;

        std::thread::sleep(Duration::from_secs(60));

        let mut conn = match get_connection(&state) {
            Ok(c) => c,
            Err(_err) => {
                continue;
            }
        };

        diesel::delete(refresh_tokens.filter(expiration.lt(Utc::now().timestamp())))
            .execute(&mut conn)
            .ok();
    }
}

fn resend_thread(bridge_state: web::Data<BridgeState>) {
    loop {
        std::thread::sleep(Duration::from_secs(1));
        let undiscovered_ports = bridge_state.port_discovery.lock().unwrap();
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
            let charger_id = port.charger_id;
            let chargers = bridge_state.charger_management_map_with_id.lock().unwrap();
            if let Some(sock) = chargers.get(&charger_id) {
                let mut sock = sock.lock().unwrap();
                sock.send_packet(ManagementPacket::CommandPacket(packet));
            }
        }
    }
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
        undiscovered_chargers: Arc::new(Mutex::new(HashMap::new())),
        lost_connections: Mutex::new(HashMap::new()),
        socket: UdpSocket::bind("0.0.0.0:51820").unwrap(),
    });

    let state_cpy = state.clone();
    std::thread::spawn(move || cleanup_thread(state_cpy));
    let bridge_state_cpy = bridge_state.clone();
    std::thread::spawn(move || resend_thread(bridge_state_cpy));

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
