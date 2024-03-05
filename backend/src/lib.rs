use std::{collections::HashMap, net::IpAddr, os::unix::net::SocketAddr, sync::Mutex};

use actix::prelude::*;
use actix_web::web::Bytes;
use db_connector::Pool;
use lettre::SmtpTransport;
use udp_server::TunnData;

pub mod error;
pub mod middleware;
pub mod models;
pub mod routes;
pub mod utils;
pub mod ws_udp_bridge;
pub mod udp_server;

#[derive(Message)]
#[rtype(result = "()")]
pub struct Message(pub Bytes);

pub struct BridgeState {
    pub pool: Pool,
    pub web_client_map: Mutex<HashMap<IpAddr, Recipient<Message>>>,
    pub charger_map: Mutex<HashMap<SocketAddr, Vec<TunnData>>>,
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
            web_client_map: Mutex::new(HashMap::new()),
            charger_map: Mutex::new(HashMap::new())
        };

        let state = web::Data::new(state);
        let bridge_state = web::Data::new(bridge_state);
        cfg.app_data(state);
        cfg.app_data(bridge_state);
    }
}
