use std::{
    collections::HashMap,
    net::{SocketAddr, UdpSocket},
    sync::{Arc, Mutex}, time::Instant,
};

use actix::prelude::*;
use db_connector::Pool;
use lettre::SmtpTransport;
use udp_server::{
    management::{ManagementResponse, RemoteConnMeta},
    socket::ManagementSocket,
};
use ws_udp_bridge::Message;

pub mod error;
pub mod middleware;
pub mod models;
pub mod routes;
pub mod udp_server;
pub mod utils;
pub mod ws_udp_bridge;

pub struct BridgeState {
    pub pool: Pool,
    pub web_client_map: Mutex<HashMap<SocketAddr, Recipient<Message>>>,
    pub undiscovered_clients: Mutex<HashMap<RemoteConnMeta, Recipient<Message>>>,
    pub charger_management_map: Arc<Mutex<HashMap<SocketAddr, Arc<Mutex<ManagementSocket>>>>>,
    pub charger_management_map_with_id: Arc<Mutex<HashMap<i32, Arc<Mutex<ManagementSocket>>>>>,
    pub port_discovery: Arc<Mutex<HashMap<ManagementResponse, Instant>>>,
    pub charger_remote_conn_map: Mutex<HashMap<RemoteConnMeta, SocketAddr>>,
    pub socket: UdpSocket,
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
    use actix_web::web::{self, ServiceConfig};
    use lettre::transport::smtp::authentication::Credentials;

    pub struct ScopeCall<F: FnMut()> {
        pub c: F,
    }
    impl<F: FnMut()> Drop for ScopeCall<F> {
        fn drop(&mut self) {
            (self.c)();
        }
    }

    #[macro_export]
    macro_rules! defer {
        ($e:expr) => {
            let _scope_call = crate::tests::ScopeCall {
                c: || -> () {
                    $e;
                },
            };
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
            pool: pool.clone(),
            jwt_secret: std::env::var("JWT_SECRET").expect("JWT_SECRET must be set!"),
            mailer,
            frontend_url: std::env::var("FRONTEND_URL").expect("FRONTEND_URL must be set!"),
        };

        let bridge_state = BridgeState {
            pool,
            charger_management_map: Arc::new(Mutex::new(HashMap::new())),
            charger_management_map_with_id: Arc::new(Mutex::new(HashMap::new())),
            port_discovery: Arc::new(Mutex::new(HashMap::new())),
            charger_remote_conn_map: Mutex::new(HashMap::new()),
            undiscovered_clients: Mutex::new(HashMap::new()),
            web_client_map: Mutex::new(HashMap::new()),
            socket: UdpSocket::bind(("0", 0)).unwrap(),
        };

        let state = web::Data::new(state);
        let bridge_state = web::Data::new(bridge_state);
        cfg.app_data(state);
        cfg.app_data(bridge_state);
    }
}
