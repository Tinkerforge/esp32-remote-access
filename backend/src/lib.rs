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
use db_connector::{models::{allowed_users::AllowedUser, chargers::Charger, verification::Verification}, Pool};
use diesel::{prelude::*, r2d2::PooledConnection, result::Error::NotFound};
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

pub fn clean_verification_tokens(conn: &mut PooledConnection<diesel::r2d2::ConnectionManager<PgConnection>>) {
    let awaiting_verification: Vec<uuid::Uuid> = {
        use db_connector::schema::verification::dsl::*;

        diesel::delete(verification.filter(expiration.lt(Utc::now().naive_utc())))
            .execute(conn)
            .ok();

        match verification
            .select(Verification::as_select())
            .load(conn)
        {
            Ok(v) => v.into_iter().map(|v| v.user).collect(),
            Err(NotFound) => Vec::new(),
            Err(err) => {
                log::error!("Failed to get Verification-Token from Database: {}", err);
                return;
            }
        }
    };
    {
        use db_connector::schema::users::dsl::*;

        let _ = diesel::delete(users.filter(id.ne_all(awaiting_verification)).filter(email_verified.eq(false)))
            .execute(conn)
            .or_else(|e| {
                log::error!("Failed to delete unverified users: {}", e);
                Ok::<usize, diesel::result::Error>(0)
            });
    }
}

// Remove chargers that dont have allowed users
pub fn clean_chargers(conn: &mut PooledConnection<diesel::r2d2::ConnectionManager<PgConnection>>) {

    // Get all chargers in database
    let chargers: Vec<Charger> = {
        use db_connector::schema::chargers::dsl::*;

        match chargers.select(Charger::as_select())
            .load(conn)
        {
            Ok(c) => c,
            Err(err) => {
                log::error!("Failed to get chargers for cleanup: {}", err);
                return
            }
        }
    };

    // Get allowed users for each charger and remove those without entries
    for charger in chargers.into_iter() {
        match AllowedUser::belonging_to(&charger)
            .select(AllowedUser::as_select())
            .load(conn)
        {
            Ok(users) => {
                if users.len() == 0 {
                    use db_connector::schema::chargers::dsl::*;

                    let _ = diesel::delete(chargers.find(&charger.id))
                        .execute(conn)
                        .or_else(|e| {
                            log::error!("Failed to remove unreferenced charger: {}", e);
                            Ok::<usize, diesel::result::Error>(0)
                        });
                }
            },
            Err(err) => {
                log::error!("Failed to get allowed user for charger cleanup: {}", err);
            }
        };
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use std::{net::Ipv4Addr, str::FromStr};

    use super::*;
    use actix_web::{
        body::BoxBody,
        dev::{Service, ServiceResponse},
        test,
        web::{self, ServiceConfig},
    };
    use ipnetwork::Ipv4Network;
    use lettre::transport::smtp::authentication::Credentials;
    use chrono::Utc;
    use db_connector::{models::{recovery_tokens::RecoveryToken, refresh_tokens::RefreshToken, users::User}, test_connection_pool};
    use rand::RngCore;
    use rand_core::OsRng;
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

    #[actix_web::test]
    async fn test_clean_verification_tokens() {
        let user_id = uuid::Uuid::new_v4();
        let user = User {
            id: user_id,
            name: user_id.to_string(),
            email: format!("{}@invalid", user_id.to_string()),
            login_key: String::new(),
            email_verified: false,
            secret: Vec::new(),
            secret_nonce: Vec::new(),
            secret_salt: Vec::new(),
            login_salt: Vec::new(),
        };

        let user2_id = uuid::Uuid::new_v4();
        let user2 = User {
            id: user2_id,
            name: user2_id.to_string(),
            email: format!("{}@invalid", user2_id.to_string()),
            login_key: String::new(),
            email_verified: false,
            secret: Vec::new(),
            secret_nonce: Vec::new(),
            secret_salt: Vec::new(),
            login_salt: Vec::new(),
        };

        let user3_id = uuid::Uuid::new_v4();
        let user3 = User {
            id: user3_id,
            name: user3_id.to_string(),
            email: format!("{}@invalid", user3_id.to_string()),
            login_key: String::new(),
            email_verified: true,
            secret: Vec::new(),
            secret_nonce: Vec::new(),
            secret_salt: Vec::new(),
            login_salt: Vec::new(),
        };

        let verify_id = uuid::Uuid::new_v4();
        let verify = Verification {
            id: verify_id,
            user: user_id,
            expiration: Utc::now().checked_sub_signed(TimeDelta::seconds(1)).unwrap().naive_utc(),
        };

        let verify2_id = uuid::Uuid::new_v4();
        let verify2 = Verification {
            id: verify2_id,
            user: user2_id,
            expiration: Utc::now().checked_add_signed(TimeDelta::seconds(1)).unwrap().naive_local(),
        };

        let pool = test_connection_pool();
        let mut conn = pool.get().unwrap();
        {
            use db_connector::schema::users::dsl::*;

            diesel::insert_into(users).values(vec![&user, &user2, &user3])
                .execute(&mut conn)
                .unwrap();
        }
        {
            use db_connector::schema::verification::dsl::*;

            diesel::insert_into(verification).values(vec![&verify, &verify2])
                .execute(&mut conn)
                .unwrap();
        }

        clean_verification_tokens(&mut conn);

        {
            use db_connector::schema::verification::dsl::*;

            let verifies: Vec<Verification> = verification
                .filter(id.eq_any(vec![&verify_id, &verify2_id]))
                .select(Verification::as_select())
                .load(&mut conn)
                .unwrap();

            assert_eq!(verifies.len(), 1);
            assert_eq!(verifies[0].id, verify2_id);
        }
        {
            use db_connector::schema::users::dsl::*;

            let u: Vec<User> = users
                .filter(id.eq_any(vec![&user_id, &user2_id, &user3_id]))
                .select(User::as_select())
                .load(&mut conn)
                .unwrap();

            assert_eq!(u.len(), 2);
            assert!(u[0].id == user2_id || u[0].id == user3_id);
            assert!(u[1].id == user3_id || u[1].id == user3_id);
        }

        {
            use db_connector::schema::verification::dsl::*;

            diesel::delete(verification.filter(id.eq_any(vec![&verify_id, &verify2_id])))
                .execute(&mut conn)
                .unwrap();
        }
        {
            use db_connector::schema::users::dsl::*;

            diesel::delete(users.filter(id.eq_any(vec![&user_id, &user2_id, &user3_id])))
                .execute(&mut conn)
                .unwrap();
        }
    }

    #[actix_web::test]
    async fn test_charger_cleanup() {
        let (mut user, _) = TestUser::random().await;
        user.login().await;
        let charger = user.add_random_charger().await;
        let charger2 = Charger {
            id: uuid::Uuid::new_v4(),
            uid: OsRng.next_u32() as i32,
            password: String::new(),
            name: None,
            management_private: String::new(),
            charger_pub: String::new(),
            wg_charger_ip: IpNetwork::V4(Ipv4Network::new(Ipv4Addr::new(123, 123, 123, 123), 24).unwrap()),
            psk: String::new(),
            wg_server_ip: IpNetwork::V4(Ipv4Network::new(Ipv4Addr::new(123, 123, 123, 123), 24).unwrap()),
            webinterface_port: 80,
            firmware_version: "2.6.6".to_string()
        };

        let pool = test_connection_pool();
        let mut conn = pool.get().unwrap();
        {
            use db_connector::schema::chargers::dsl::*;

            diesel::insert_into(chargers)
                .values(&charger2)
                .execute(&mut conn)
                .unwrap();
        }

        clean_chargers(&mut conn);

        let charger_id = uuid::Uuid::from_str(&charger.uuid).unwrap();
        let chargers: Vec<Charger> = {
            use db_connector::schema::chargers::dsl::*;

            chargers.filter(id.eq_any(vec![&charger_id, &charger2.id]))
                .select(Charger::as_select())
                .load(&mut conn)
                .unwrap()
        };

        assert_eq!(chargers.len(), 1);
        assert_eq!(chargers[0].id, charger_id);
    }
}
