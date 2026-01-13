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

use actix::clock::interval;
use actix_web::{get, rt, web, HttpRequest, HttpResponse, Responder};
use actix_ws::AggregatedMessage;
use base64::{prelude::BASE64_STANDARD, Engine};
use db_connector::models::{allowed_users::AllowedUser, chargers::Charger};
use diesel::{prelude::*, result::Error::NotFound};
use futures_util::future::Either;
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};
use utoipa::ToSchema;

use crate::{
    error::Error,
    routes::user::get_user,
    utils::{get_connection, web_block_unpacked},
    AppState, BridgeState,
};

const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(5);
const CLIENT_TIMEOUT: Duration = Duration::from_secs(15);

#[derive(Serialize, Deserialize, ToSchema, Clone)]
pub enum ChargerStatus {
    Disconnected = 0,
    Connected = 1,
}

#[derive(Serialize, Deserialize, ToSchema, Clone)]
pub struct GetChargerSchema {
    id: String,
    uid: i32,
    name: String,
    note: Option<String>,
    status: ChargerStatus,
    port: i32,
    valid: bool,
    last_state_change: Option<i64>,
    firmware_version: String,
}

#[derive(Serialize, Clone)]
#[serde(tag = "type")]
pub enum StateUpdateMessage {
    #[serde(rename = "state_change")]
    StateChange { chargers: Vec<GetChargerSchema> },
}

pub async fn fetch_chargers(
    state: &web::Data<AppState>,
    uid: uuid::Uuid,
    bridge_state: &web::Data<BridgeState>,
) -> Result<Vec<GetChargerSchema>, actix_web::Error> {
    use db_connector::schema::allowed_users::dsl as allowed_users;
    use db_connector::schema::chargers::dsl as chargers;

    let user = get_user(state, uid).await?;

    let mut conn = get_connection(state)?;
    let charger: Vec<(Charger, AllowedUser)> = web_block_unpacked(move || {
        let allowed_users_list: Vec<AllowedUser> = match AllowedUser::belonging_to(&user)
            .select(AllowedUser::as_select())
            .load(&mut conn)
        {
            Ok(d) => d,
            Err(NotFound) => Vec::new(),
            Err(err) => {
                log::error!("Failed to load allowed users: {err}");
                return Err(Error::InternalError);
            }
        };

        let charger_ids = AllowedUser::belonging_to(&user).select(allowed_users::charger_id);
        let chargers_list: Vec<Charger> = match chargers::chargers
            .filter(chargers::id.eq_any(charger_ids))
            .select(Charger::as_select())
            .load(&mut conn)
        {
            Ok(v) => v,
            Err(err) => {
                log::error!("Failed to load chargers: {err}");
                return Err(Error::InternalError);
            }
        };

        let chargers_by_users: Vec<(Charger, AllowedUser)> = allowed_users_list
            .grouped_by(&chargers_list)
            .into_iter()
            .zip(chargers_list)
            .filter_map(|(allowed_users_for_charger, charger)| {
                allowed_users_for_charger
                    .first()
                    .map(|au| (charger, au.clone()))
            })
            .collect();

        Ok(chargers_by_users)
    })
    .await?;

    let charger_map = bridge_state.charger_management_map_with_id.lock().await;
    let charger = charger
        .into_iter()
        .map(|(c, allowed_user)| {
            let status = if charger_map.contains_key(&c.id) {
                ChargerStatus::Connected
            } else {
                ChargerStatus::Disconnected
            };

            let name = if let Some(name) = allowed_user.name {
                name
            } else if let Some(name) = c.name {
                BASE64_STANDARD.encode(name)
            } else {
                String::new()
            };

            GetChargerSchema {
                id: c.id.to_string(),
                uid: c.uid,
                name,
                note: allowed_user.note,
                status,
                port: c.webinterface_port,
                valid: allowed_user.valid,
                last_state_change: c.last_state_change.map(|ts| ts.and_utc().timestamp()),
                firmware_version: c.firmware_version,
            }
        })
        .collect::<Vec<GetChargerSchema>>();

    Ok(charger)
}

/// WebSocket endpoint for get_chargers with live state updates
#[get("/get_chargers")]
pub async fn get_chargers(
    req: HttpRequest,
    stream: web::Payload,
    state: web::Data<AppState>,
    uid: crate::models::uuid::Uuid,
    bridge_state: web::Data<BridgeState>,
) -> Result<impl Responder, actix_web::Error> {
    handle_websocket(req, state, uid, bridge_state, stream).await
}

async fn handle_websocket(
    req: HttpRequest,
    state: web::Data<AppState>,
    uid: crate::models::uuid::Uuid,
    bridge_state: web::Data<BridgeState>,
    stream: web::Payload,
) -> Result<HttpResponse, actix_web::Error> {
    let (resp, mut session, ws_stream) = actix_ws::handle(&req, stream)?;
    let mut ws_stream = ws_stream
        .aggregate_continuations()
        .max_continuation_size(2_usize.pow(20));

    let user_id: uuid::Uuid = uid.into();

    if resp.status() == 101 {
        let mut state_update_clients = bridge_state.state_update_clients.lock().await;
        state_update_clients.insert(user_id, session.clone());
    }

    rt::spawn(async move {
        let mut last_heartbeat = Instant::now();
        let mut interval_timer = interval(HEARTBEAT_INTERVAL);

        // Send initial charger list
        if let Ok(chargers) = fetch_chargers(&state, user_id, &bridge_state).await {
            if let Ok(json) = serde_json::to_string(&chargers) {
                let _ = session.text(json).await;
            }
        }

        loop {
            let tick = interval_timer.tick();
            actix_web::rt::pin!(tick);

            match futures_util::future::select(ws_stream.next(), tick).await {
                Either::Left((Some(Ok(AggregatedMessage::Close(_))), _)) => break,
                Either::Left((Some(Ok(AggregatedMessage::Ping(msg))), _)) => {
                    let _ = session.pong(&msg).await;
                    last_heartbeat = Instant::now();
                }
                Either::Left((Some(Ok(AggregatedMessage::Pong(_))), _)) => {
                    last_heartbeat = Instant::now();
                }
                Either::Left((Some(Ok(_msg)), _)) => {
                    // Ignore other messages
                }
                Either::Left((Some(Err(_err)), _)) => {
                    log::error!("Websocket Error during get_chargers connection: {_err:?}");
                    break;
                }
                Either::Left((None, _)) => break,
                Either::Right(_) => {
                    if Instant::now().duration_since(last_heartbeat) > CLIENT_TIMEOUT {
                        log::debug!("get_chargers WebSocket client quietly quit.");
                        break;
                    }
                    let _ = session.ping(b"").await;
                }
            }
        }

        let mut state_update_clients = bridge_state.state_update_clients.lock().await;
        state_update_clients.remove(&user_id);
        let _ = session.close(None).await;
    });

    Ok(resp)
}

#[cfg(test)]
mod tests {
    use actix_web::web;
    use base64::{prelude::BASE64_STANDARD, Engine};
    use db_connector::{models::users::User, test_connection_pool};
    use diesel::prelude::*;
    use rand::TryRngCore;
    use rand_core::OsRng;
    use std::str::FromStr;

    use super::*;
    use crate::{
        routes::{charger::allow_user::UserAuth, user::tests::TestUser},
        tests::{create_test_bridge_state, create_test_state},
    };

    /// Helper function to get AppState and BridgeState for testing fetch_chargers directly
    fn get_test_state() -> (web::Data<AppState>, web::Data<BridgeState>) {
        let pool = test_connection_pool();
        let state = create_test_state(Some(pool.clone()));
        let bridge_state = create_test_bridge_state(Some(pool));
        (state, bridge_state)
    }

    /// Helper function to get the user UUID from the email address
    fn get_user_uuid_from_email(email_addr: &str) -> uuid::Uuid {
        use db_connector::schema::users::dsl::*;

        let pool = test_connection_pool();
        let mut conn = pool.get().unwrap();

        users
            .filter(email.eq(email_addr))
            .select(User::as_select())
            .get_result(&mut conn)
            .expect("Failed to find user")
            .id
    }

    /// Test if only the chargers the user has access to will be returned.
    #[actix_web::test]
    async fn test_get_chargers() {
        let (mut user1, _) = TestUser::random().await;
        let (mut user2, _) = TestUser::random().await;
        user1.login().await;
        user2.login().await;
        for _ in 0..5 {
            let _ = user1.add_random_charger().await;
            let charger = user2.add_random_charger().await;
            user2
                .allow_user(
                    &user1.mail,
                    UserAuth::LoginKey(BASE64_STANDARD.encode(user1.get_login_key().await)),
                    &charger,
                )
                .await;
        }
        for _ in 0..5 {
            let uuid = OsRng.try_next_u32().unwrap() as i32;
            user2.add_charger(uuid).await;
        }

        let (state, bridge_state) = get_test_state();
        let user_id = get_user_uuid_from_email(&user1.mail);

        let resp = fetch_chargers(&state, user_id, &bridge_state)
            .await
            .expect("fetch_chargers failed");
        assert_eq!(resp.len(), 10);
    }

    #[actix_web::test]
    async fn test_get_not_existing_chargers() {
        let (mut user1, _) = TestUser::random().await;
        user1.login().await;

        let (state, bridge_state) = get_test_state();
        let user_id = get_user_uuid_from_email(&user1.mail);

        let resp = fetch_chargers(&state, user_id, &bridge_state)
            .await
            .expect("fetch_chargers failed");
        assert_eq!(resp.len(), 0);
    }

    /// Test that the race condition between fetching allowed_users and chargers is handled.
    ///
    /// This tests the fix for a race condition where:
    /// 1. allowed_users are fetched for a user
    /// 2. chargers are fetched based on those allowed_users
    /// 3. Between those two queries, an allowed_user entry could be deleted
    /// 4. This would result in grouped_by returning an empty array for that charger
    ///
    /// The fix uses filter_map with .first() instead of directly accessing [0],
    /// which would panic on an empty array.
    #[actix_web::test]
    async fn test_race_condition_allowed_user_deleted_between_queries() {
        let (mut user1, _) = TestUser::random().await;
        user1.login().await;

        // Add two chargers - we'll delete the allowed_user entry for one of them
        let charger1 = user1.add_random_charger().await;
        let charger2 = user1.add_random_charger().await;

        // Directly delete the allowed_user entry for charger1 from the database
        // This simulates the race condition where another request deletes the
        // allowed_user between fetching allowed_users and joining with chargers
        {
            use db_connector::schema::allowed_users::dsl::*;

            let charger1_uuid = uuid::Uuid::from_str(&charger1.uuid).unwrap();
            let pool = test_connection_pool();
            let mut conn = pool.get().unwrap();

            diesel::delete(allowed_users.filter(charger_id.eq(charger1_uuid)))
                .execute(&mut conn)
                .expect("Failed to delete allowed_user entry");
        }

        let (state, bridge_state) = get_test_state();
        let user_id = get_user_uuid_from_email(&user1.mail);

        // This should not panic even though charger1 has no allowed_users entry
        let resp = fetch_chargers(&state, user_id, &bridge_state)
            .await
            .expect("fetch_chargers failed");

        // Only charger2 should be returned since charger1's allowed_user was deleted
        assert_eq!(resp.len(), 1);
        assert_eq!(resp[0].id, charger2.uuid);
    }
}
