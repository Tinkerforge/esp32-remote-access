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
    sync::Arc,
    time::Instant,
};

use actix_ws::Session;
pub use boringtun::*;
use chrono::{TimeDelta, Utc};
use db_connector::{
    models::{
        allowed_users::AllowedUser, chargers::Charger, users::User, verification::Verification,
    },
    Pool,
};
use diesel::{prelude::*, r2d2::PooledConnection, result::Error::NotFound};
use futures_util::lock::Mutex;
use ipnetwork::IpNetwork;
use lettre::SmtpTransport;
use serde::{ser::SerializeStruct, Serialize};
use udp_server::{
    management::RemoteConnMeta, packet::ManagementResponseV2, socket::ManagementSocket,
};

pub mod branding;
pub mod error;
pub mod hasher;
pub mod middleware;
pub mod models;
pub mod rate_limit;
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
    pub web_client_map: Mutex<HashMap<SocketAddr, Session>>,
    pub undiscovered_clients: Mutex<HashMap<RemoteConnMeta, Session>>,
    pub charger_management_map: Arc<Mutex<HashMap<SocketAddr, Arc<Mutex<ManagementSocket>>>>>,
    pub charger_management_map_with_id:
        Arc<Mutex<HashMap<uuid::Uuid, Arc<Mutex<ManagementSocket>>>>>,
    pub port_discovery: Arc<Mutex<HashMap<ManagementResponseV2, Instant>>>,
    pub charger_remote_conn_map: Mutex<HashMap<RemoteConnMeta, SocketAddr>>,
    pub undiscovered_chargers: Arc<Mutex<HashMap<IpNetwork, HashSet<DiscoveryCharger>>>>,
    pub lost_connections: Mutex<HashMap<uuid::Uuid, Vec<(i32, Session)>>>,
    pub socket: Arc<UdpSocket>,
    pub state_update_clients: Mutex<HashMap<uuid::Uuid, Session>>,
}

pub struct AppState {
    pub pool: Pool,
    pub jwt_secret: String,
    pub mailer: Option<SmtpTransport>,
    pub frontend_url: String,
    pub sender_email: String,
    pub sender_name: String,
    pub brand: crate::branding::Brand,
    pub keys_in_use: Mutex<HashSet<uuid::Uuid>>,
    pub hasher: crate::hasher::HasherManager,
}

pub fn clean_recovery_tokens(
    conn: &mut PooledConnection<diesel::r2d2::ConnectionManager<PgConnection>>,
) {
    use db_connector::schema::recovery_tokens::dsl::*;

    if let Some(time) = Utc::now().checked_sub_signed(TimeDelta::hours(6)) {
        diesel::delete(recovery_tokens.filter(created.lt(time.timestamp())))
            .execute(conn)
            .ok();
    }
}

pub fn clean_refresh_tokens(
    conn: &mut PooledConnection<diesel::r2d2::ConnectionManager<PgConnection>>,
) {
    use db_connector::schema::refresh_tokens::dsl::*;

    diesel::delete(refresh_tokens.filter(expiration.lt(Utc::now().timestamp())))
        .execute(conn)
        .ok();
}

pub fn clean_verification_tokens(
    conn: &mut PooledConnection<diesel::r2d2::ConnectionManager<PgConnection>>,
) {
    // Remove all verification tokens that are expired
    let awaiting_verification: Vec<uuid::Uuid> = {
        use db_connector::schema::verification::dsl::*;

        diesel::delete(verification.filter(expiration.lt(Utc::now().naive_utc())))
            .execute(conn)
            .ok();

        match verification.select(Verification::as_select()).load(conn) {
            Ok(v) => v.into_iter().map(|v| v.user).collect(),
            Err(NotFound) => Vec::new(),
            Err(err) => {
                log::error!("Failed to get Verification-Token from Database: {err}");
                return;
            }
        }
    };

    // Reset all changed email addresses that timeouted
    let awaiting_verification: Vec<uuid::Uuid> = {
        use db_connector::schema::users::dsl as users;

        let users: Vec<User> = match users::users
            .filter(users::id.ne_all(&awaiting_verification))
            .filter(users::email_verified.eq(false))
            .filter(users::old_email.is_not_null())
            .select(User::as_select())
            .load(conn)
        {
            Ok(u) => u,
            Err(NotFound) => Vec::new(),
            Err(err) => {
                log::error!("Failed to get users for cleanup: {err}");
                return;
            }
        };
        for user in users.iter() {
            let _ = diesel::update(users::users.find(&user.id))
                .set((
                    users::email.eq(&user.old_email.as_ref().unwrap()),
                    users::delivery_email.eq(&user.old_delivery_email),
                    users::old_email.eq::<Option<String>>(None),
                    users::old_delivery_email.eq::<Option<String>>(None),
                    users::email_verified.eq(true),
                ))
                .execute(conn)
                .or_else(|e| {
                    log::error!("Failed to update user: {e}");
                    Ok::<usize, diesel::result::Error>(0)
                });
        }

        awaiting_verification
            .into_iter()
            .filter(|v| !users.iter().any(|u| &u.id == v))
            .collect()
    };

    // Remove all users that are not verified and have no verification token
    {
        use db_connector::schema::users::dsl::*;

        let _ = diesel::delete(
            users
                .filter(id.ne_all(awaiting_verification))
                .filter(email_verified.eq(false)),
        )
        .execute(conn)
        .or_else(|e| {
            log::error!("Failed to delete unverified users: {e}");
            Ok::<usize, diesel::result::Error>(0)
        });
    }
}

// Remove chargers that dont have allowed users
pub fn clean_chargers(conn: &mut PooledConnection<diesel::r2d2::ConnectionManager<PgConnection>>) {
    // Get all chargers in database
    let chargers: Vec<Charger> = {
        use db_connector::schema::chargers::dsl::*;

        match chargers.select(Charger::as_select()).load(conn) {
            Ok(c) => c,
            Err(err) => {
                log::error!("Failed to get chargers for cleanup: {err}");
                return;
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
                if users.is_empty() {
                    use db_connector::schema::chargers::dsl::*;

                    let _ = diesel::delete(chargers.find(&charger.id))
                        .execute(conn)
                        .or_else(|e| {
                            log::error!("Failed to remove unreferenced charger: {e}");
                            Ok::<usize, diesel::result::Error>(0)
                        });
                }
            }
            Err(err) => {
                log::error!("Failed to get allowed user for charger cleanup: {err}");
            }
        };
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use std::{net::Ipv4Addr, num::NonZeroUsize, str::FromStr};

    use super::*;
    use actix_web::{
        body::BoxBody,
        dev::{Service, ServiceResponse},
        test,
        web::{self, ServiceConfig},
    };
    use chrono::Utc;
    use db_connector::{
        models::{recovery_tokens::RecoveryToken, refresh_tokens::RefreshToken, users::User},
        test_connection_pool,
    };
    use diesel::r2d2::ConnectionManager;
    use ipnetwork::Ipv4Network;
    use lru::LruCache;
    use rand::TryRngCore;
    use rand_core::OsRng;
    use rate_limit::{ChargerRateLimiter, LoginRateLimiter};
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
            let _scope_call = $crate::tests::ScopeCall {
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

    pub fn create_test_state(
        pool: Option<diesel::r2d2::Pool<ConnectionManager<PgConnection>>>,
    ) -> web::Data<AppState> {
        let pool = pool.unwrap_or_else(db_connector::test_connection_pool);

        let state = AppState {
            pool: pool.clone(),
            jwt_secret: std::env::var("JWT_SECRET").expect("JWT_SECRET must be set!"),
            mailer: None,
            frontend_url: std::env::var("FRONTEND_URL").expect("FRONTEND_URL must be set!"),
            sender_email: String::new(),
            sender_name: String::new(),
            brand: crate::branding::Brand::default(),
            keys_in_use: Mutex::new(HashSet::new()),
            hasher: crate::hasher::HasherManager::default(),
        };

        web::Data::new(state)
    }

    pub async fn mark_keys_as_in_use(state: &web::Data<AppState>, key_ids: Vec<uuid::Uuid>) {
        let mut keys_in_use = state.keys_in_use.lock().await;
        for key_id in key_ids {
            keys_in_use.insert(key_id);
        }
    }

    pub async fn get_charger_key_ids(
        state: &web::Data<AppState>,
        charger_uuid: uuid::Uuid,
    ) -> Vec<uuid::Uuid> {
        use db_connector::schema::wg_keys::dsl::*;

        let mut conn = state.pool.get().unwrap();
        wg_keys
            .filter(charger_id.eq(charger_uuid))
            .select(id)
            .load::<uuid::Uuid>(&mut conn)
            .unwrap_or_default()
    }

    pub fn create_test_bridge_state(
        pool: Option<diesel::r2d2::Pool<ConnectionManager<PgConnection>>>,
    ) -> web::Data<BridgeState> {
        let pool = pool.unwrap_or_else(db_connector::test_connection_pool);

        let bridge_state = BridgeState {
            pool: pool.clone(),
            charger_management_map: Arc::new(Mutex::new(HashMap::new())),
            charger_management_map_with_id: Arc::new(Mutex::new(HashMap::new())),
            port_discovery: Arc::new(Mutex::new(HashMap::new())),
            charger_remote_conn_map: Mutex::new(HashMap::new()),
            undiscovered_clients: Mutex::new(HashMap::new()),
            web_client_map: Mutex::new(HashMap::new()),
            undiscovered_chargers: Arc::new(Mutex::new(HashMap::new())),
            lost_connections: Mutex::new(HashMap::new()),
            socket: Arc::new(UdpSocket::bind(("0", 0)).unwrap()),
            state_update_clients: Mutex::new(HashMap::new()),
        };

        web::Data::new(bridge_state)
    }

    pub fn configure(cfg: &mut ServiceConfig) {
        let pool = db_connector::test_connection_pool();

        let bridge_state = BridgeState {
            pool: pool.clone(),
            charger_management_map: Arc::new(Mutex::new(HashMap::new())),
            charger_management_map_with_id: Arc::new(Mutex::new(HashMap::new())),
            port_discovery: Arc::new(Mutex::new(HashMap::new())),
            charger_remote_conn_map: Mutex::new(HashMap::new()),
            undiscovered_clients: Mutex::new(HashMap::new()),
            web_client_map: Mutex::new(HashMap::new()),
            undiscovered_chargers: Arc::new(Mutex::new(HashMap::new())),
            lost_connections: Mutex::new(HashMap::new()),
            socket: Arc::new(UdpSocket::bind(("0", 0)).unwrap()),
            state_update_clients: Mutex::new(HashMap::new()),
        };

        let cache: web::Data<std::sync::Mutex<LruCache<String, Vec<u8>>>> = web::Data::new(
            std::sync::Mutex::new(LruCache::new(NonZeroUsize::new(10000).unwrap())),
        );

        let state = create_test_state(Some(pool));
        let bridge_state = web::Data::new(bridge_state);
        let login_rate_limiter = web::Data::new(LoginRateLimiter::new());
        let charger_rate_limiter = web::Data::new(ChargerRateLimiter::new());
        let general_rate_limiter = web::Data::new(rate_limit::IPRateLimiter::new());
        cfg.app_data(login_rate_limiter);
        cfg.app_data(charger_rate_limiter);
        cfg.app_data(general_rate_limiter);
        cfg.app_data(state);
        cfg.app_data(bridge_state);
        cfg.app_data(cache);
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
            created: Utc::now()
                .checked_sub_signed(TimeDelta::hours(6))
                .unwrap()
                .timestamp()
                + 1,
        };
        let token2 = RecoveryToken {
            id: uuid::Uuid::new_v4(),
            user_id: uid,
            created: Utc::now()
                .checked_sub_signed(TimeDelta::hours(6))
                .unwrap()
                .timestamp()
                - 1,
        };
        let token3 = RecoveryToken {
            id: uuid::Uuid::new_v4(),
            user_id: uid,
            created: Utc::now()
                .checked_sub_signed(TimeDelta::hours(7))
                .unwrap()
                .timestamp(),
        };

        diesel::insert_into(recovery_tokens)
            .values(vec![&token1, &token2, &token3])
            .execute(&mut conn)
            .unwrap();

        clean_recovery_tokens(&mut conn);

        let tokens: Vec<RecoveryToken> = recovery_tokens
            .filter(user_id.eq(uid))
            .select(RecoveryToken::as_select())
            .load(&mut conn)
            .unwrap();

        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].id, token1_id);

        diesel::delete(recovery_tokens.filter(user_id.eq(uid)))
            .execute(&mut conn)
            .unwrap();
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

        diesel::insert_into(refresh_tokens)
            .values(vec![&token1, &token2])
            .execute(&mut conn)
            .unwrap();

        clean_refresh_tokens(&mut conn);

        let tokens: Vec<RefreshToken> = refresh_tokens
            .filter(user_id.eq(uid))
            .select(RefreshToken::as_select())
            .load(&mut conn)
            .unwrap();

        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].id, token1_id);

        diesel::delete(refresh_tokens.filter(user_id.eq(uid)))
            .execute(&mut conn)
            .unwrap();
    }

    #[actix_web::test]
    async fn test_clean_verification_tokens() {
        let user_id = uuid::Uuid::new_v4();
        let email = format!("{user_id}@invalid");
        let user = User {
            id: user_id,
            name: user_id.to_string(),
            email: email.clone(),
            login_key: String::new(),
            email_verified: false,
            secret: Vec::new(),
            secret_nonce: Vec::new(),
            secret_salt: Vec::new(),
            login_salt: Vec::new(),
            delivery_email: Some(email.clone()),
            old_email: None,
            old_delivery_email: None,
        };

        let user2_id = uuid::Uuid::new_v4();
        let user2 = User {
            id: user2_id,
            name: user2_id.to_string(),
            email: format!("{user2_id}@invalid"),
            login_key: String::new(),
            email_verified: false,
            secret: Vec::new(),
            secret_nonce: Vec::new(),
            secret_salt: Vec::new(),
            login_salt: Vec::new(),
            delivery_email: None,
            old_email: None,
            old_delivery_email: None,
        };

        let user3_id = uuid::Uuid::new_v4();
        let user3 = User {
            id: user3_id,
            name: user3_id.to_string(),
            email: format!("{user3_id}@invalid"),
            login_key: String::new(),
            email_verified: true,
            secret: Vec::new(),
            secret_nonce: Vec::new(),
            secret_salt: Vec::new(),
            login_salt: Vec::new(),
            delivery_email: None,
            old_email: None,
            old_delivery_email: None,
        };

        let user4_id = uuid::Uuid::new_v4();
        let user4 = User {
            id: user4_id,
            name: user4_id.to_string(),
            email: format!("{user4_id}@invalid"),
            login_key: String::new(),
            email_verified: false,
            secret: Vec::new(),
            secret_nonce: Vec::new(),
            secret_salt: Vec::new(),
            login_salt: Vec::new(),
            delivery_email: Some(email.clone()),
            old_email: Some(email.clone()),
            old_delivery_email: Some(email.clone()),
        };

        let verify_id = uuid::Uuid::new_v4();
        let verify = Verification {
            id: verify_id,
            user: user_id,
            expiration: Utc::now()
                .checked_sub_signed(TimeDelta::seconds(1))
                .unwrap()
                .naive_utc(),
        };

        let verify2_id = uuid::Uuid::new_v4();
        let verify2 = Verification {
            id: verify2_id,
            user: user2_id,
            expiration: Utc::now()
                .checked_add_signed(TimeDelta::seconds(1))
                .unwrap()
                .naive_local(),
        };

        let verify4_id = uuid::Uuid::new_v4();
        let verify4 = Verification {
            id: verify4_id,
            user: user4_id,
            expiration: Utc::now()
                .checked_sub_signed(TimeDelta::seconds(1))
                .unwrap()
                .naive_utc(),
        };

        let cleanup = || {
            let pool = test_connection_pool();
            let mut conn = pool.get().unwrap();
            {
                use db_connector::schema::verification::dsl::*;

                diesel::delete(verification.filter(id.eq_any(vec![
                    &verify_id,
                    &verify2_id,
                    &verify4_id,
                ])))
                .execute(&mut conn)
                .unwrap();
            }
            {
                use db_connector::schema::users::dsl::*;

                diesel::delete(
                    users.filter(id.eq_any(vec![&user_id, &user2_id, &user3_id, &user4_id])),
                )
                .execute(&mut conn)
                .unwrap();
            }
        };
        defer!(cleanup());

        let pool = test_connection_pool();
        let mut conn = pool.get().unwrap();
        {
            use db_connector::schema::users::dsl::*;

            diesel::insert_into(users)
                .values(vec![&user, &user2, &user3, &user4])
                .execute(&mut conn)
                .unwrap();
        }
        {
            use db_connector::schema::verification::dsl::*;

            diesel::insert_into(verification)
                .values(vec![&verify, &verify2, &verify4])
                .execute(&mut conn)
                .unwrap();
        }

        clean_verification_tokens(&mut conn);

        {
            use db_connector::schema::verification::dsl::*;

            let verifies: Vec<Verification> = verification
                .filter(id.eq_any(vec![&verify_id, &verify2_id, &verify4_id]))
                .select(Verification::as_select())
                .load(&mut conn)
                .unwrap();

            assert_eq!(verifies.len(), 1);
            assert_eq!(verifies[0].id, verify2_id);
        }
        let user = {
            use db_connector::schema::users::dsl::*;

            let u: Vec<User> = users
                .filter(id.eq_any(vec![&user_id, &user2_id, &user3_id, &user4_id]))
                .select(User::as_select())
                .load(&mut conn)
                .unwrap();

            assert_eq!(u.len(), 3);
            assert!(u.iter().any(|u| u.id == user2_id));
            assert!(u.iter().any(|u| u.id == user3_id));
            assert!(u.iter().any(|u| u.id == user4_id));

            u.into_iter().find(|u| u.id == user4_id).unwrap()
        };

        assert_eq!(user.email, email);
    }

    #[actix_web::test]
    async fn test_charger_cleanup() {
        let (mut user, _) = TestUser::random().await;
        user.login().await;
        let charger = user.add_random_charger().await;
        let charger2 = Charger {
            id: uuid::Uuid::new_v4(),
            uid: OsRng.try_next_u32().unwrap() as i32,
            password: String::new(),
            name: None,
            management_private: String::new(),
            charger_pub: String::new(),
            wg_charger_ip: IpNetwork::V4(
                Ipv4Network::new(Ipv4Addr::new(123, 123, 123, 123), 24).unwrap(),
            ),
            psk: String::new(),
            wg_server_ip: IpNetwork::V4(
                Ipv4Network::new(Ipv4Addr::new(123, 123, 123, 123), 24).unwrap(),
            ),
            webinterface_port: 80,
            firmware_version: "2.6.6".to_string(),
            last_state_change: None,
            device_type: None,
            mtu: None,
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

            chargers
                .filter(id.eq_any(vec![&charger_id, &charger2.id]))
                .select(Charger::as_select())
                .load(&mut conn)
                .unwrap()
        };

        assert_eq!(chargers.len(), 1);
        assert_eq!(chargers[0].id, charger_id);
    }
}
