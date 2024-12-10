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
    collections::{HashMap, HashSet},
    net::{SocketAddr, UdpSocket},
    sync::{Arc, Mutex},
    time::Instant,
};

use actix::prelude::*;
pub use boringtun::*;
use chrono::{TimeDelta, Utc};
use db_connector::Pool;
use diesel::{prelude::*, r2d2::PooledConnection};
use ipnetwork::IpNetwork;
use lettre::SmtpTransport;
use serde::{ser::SerializeStruct, Serialize};
use udp_server::{
    management::RemoteConnMeta, packet::ManagementResponseV2, socket::ManagementSocket,
};
use ws_udp_bridge::Message;

pub mod error;
pub mod middleware;
pub mod models;
pub mod routes;
pub mod udp_server;
pub mod utils;
pub mod ws_udp_bridge;

#[derive(Hash, PartialEq, Eq, Clone, Debug)]
pub struct DiscoveryCharger {
    pub id: uuid::Uuid,
    pub last_request: Instant,
}

impl Serialize for DiscoveryCharger {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut s = serializer.serialize_struct("DiscoveryCharger", 2)?;
        s.serialize_field("id", &self.id.to_string())?;
        s.serialize_field("alive_since", &self.last_request.elapsed().as_secs())?;
        s.end()
    }
}

pub struct BridgeState {
    pub pool: Pool,
    pub web_client_map: Mutex<HashMap<SocketAddr, Recipient<Message>>>,
    pub undiscovered_clients: Mutex<HashMap<RemoteConnMeta, Recipient<Message>>>,
    pub charger_management_map: Arc<Mutex<HashMap<SocketAddr, Arc<Mutex<ManagementSocket>>>>>,
    pub charger_management_map_with_id: Arc<Mutex<HashMap<uuid::Uuid, Arc<Mutex<ManagementSocket>>>>>,
    pub port_discovery: Arc<Mutex<HashMap<ManagementResponseV2, Instant>>>,
    pub charger_remote_conn_map: Mutex<HashMap<RemoteConnMeta, SocketAddr>>,
    pub undiscovered_chargers: Arc<Mutex<HashMap<IpNetwork, HashSet<DiscoveryCharger>>>>,
    pub lost_connections: Mutex<HashMap<uuid::Uuid, Vec<(i32, Recipient<Message>)>>>,
    pub socket: UdpSocket,
}

pub struct AppState {
    pub pool: Pool,
    pub jwt_secret: String,
    pub mailer: SmtpTransport,
    pub frontend_url: String,
}

pub fn clean_recovery_tokens(conn: &mut PooledConnection<diesel::r2d2::ConnectionManager<PgConnection>>) {
    use db_connector::schema::recovery_tokens::dsl::*;

    if let Some(time) = Utc::now().checked_sub_signed(TimeDelta::hours(1)) {
        diesel::delete(recovery_tokens.filter(created.lt(time.timestamp())))
            .execute(conn)
            .ok();
    }
}

pub fn clean_refresh_tokens(conn: &mut PooledConnection<diesel::r2d2::ConnectionManager<PgConnection>>) {
    use db_connector::schema::refresh_tokens::dsl::*;

    diesel::delete(refresh_tokens.filter(expiration.lt(Utc::now().timestamp())))
        .execute(conn)
        .ok();
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use actix_web::{
        body::BoxBody,
        dev::{Service, ServiceResponse},
        test,
        web::{self, ServiceConfig},
    };
    use lettre::transport::smtp::authentication::Credentials;
    use chrono::Utc;
    use db_connector::{models::{recovery_tokens::RecoveryToken, refresh_tokens::RefreshToken}, test_connection_pool};
    use routes::user::tests::{get_test_uuid, TestUser};

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

    pub async fn call_service<S, R, E>(app: &S, req: R) -> S::Response
    where
        S: Service<R, Response = ServiceResponse<BoxBody>, Error = E>,
        E: std::fmt::Debug + Into<actix_web::Error>,
    {
        match test::try_call_service(app, req).await {
            Ok(r) => r,
            Err(_err) => {
                ServiceResponse::from_err(_err, test::TestRequest::default().to_http_request())
            }
        }
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
            undiscovered_chargers: Arc::new(Mutex::new(HashMap::new())),
            lost_connections: Mutex::new(HashMap::new()),
            socket: UdpSocket::bind(("0", 0)).unwrap(),
        };

        let state = web::Data::new(state);
        let bridge_state = web::Data::new(bridge_state);
        cfg.app_data(state);
        cfg.app_data(bridge_state);
    }

    #[actix_web::test]
    async fn test_clean_recovery_tokens() {
        use db_connector::schema::recovery_tokens::dsl::*;

        let (user, _) = TestUser::random().await;

        let pool = test_connection_pool();
        let mut conn = pool.get().unwrap();

        let uid = get_test_uuid(&user.mail).unwrap();
        let token1_id = uuid::Uuid::new_v4();
        let token1 = RecoveryToken {
            id: token1_id,
            user_id: uid,
            created: Utc::now().checked_sub_signed(TimeDelta::hours(1)).unwrap().timestamp() + 1,
        };
        let token2 = RecoveryToken {
            id: uuid::Uuid::new_v4(),
            user_id: uid,
            created: Utc::now().checked_sub_signed(TimeDelta::hours(1)).unwrap().timestamp() - 1,
        };
        let token3 = RecoveryToken {
            id: uuid::Uuid::new_v4(),
            user_id: uid,
            created: Utc::now().checked_sub_signed(TimeDelta::hours(2)).unwrap().timestamp(),
        };

        diesel::insert_into(recovery_tokens).values(vec![&token1, &token2, &token3])
            .execute(&mut conn).unwrap();

        clean_recovery_tokens(&mut conn);

        let tokens: Vec<RecoveryToken> = recovery_tokens.filter(user_id.eq(uid))
            .select(RecoveryToken::as_select())
            .load(&mut conn)
            .unwrap();

        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].id, token1_id);

        diesel::delete(recovery_tokens.filter(user_id.eq(uid))).execute(&mut conn).unwrap();
    }

    #[actix_web::test]
    async fn test_clean_refresh_tokens() {
        use db_connector::schema::refresh_tokens::dsl::*;

        let (user, _) = TestUser::random().await;

        let pool = test_connection_pool();
        let mut conn = pool.get().unwrap();

        let uid = get_test_uuid(&user.mail).unwrap();
        let token1_id = uuid::Uuid::new_v4();
        let token1 = RefreshToken {
            id: token1_id,
            user_id: uid,
            expiration: Utc::now().timestamp() + 1,
        };
        let token2 = RefreshToken {
            id: uuid::Uuid::new_v4(),
            user_id: uid,
            expiration: Utc::now().timestamp() - 1,
        };

        diesel::insert_into(refresh_tokens).values(vec![&token1, &token2])
            .execute(&mut conn).unwrap();

        clean_refresh_tokens(&mut conn);

        let tokens: Vec<RefreshToken> = refresh_tokens.filter(user_id.eq(uid))
            .select(RefreshToken::as_select())
            .load(&mut conn)
            .unwrap();

        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].id, token1_id);

        diesel::delete(refresh_tokens.filter(user_id.eq(uid))).execute(&mut conn).unwrap();
    }
}
