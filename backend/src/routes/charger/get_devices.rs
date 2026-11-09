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
use diesel::{ExpressionMethods, QueryDsl, SelectableHelper};
use diesel_async::RunQueryDsl as _;
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;
use utoipa::ToSchema;

use crate::{error::Error, routes::user::get_user, utils::get_connection, AppState, BridgeState};

const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(5);
const CLIENT_TIMEOUT: Duration = Duration::from_secs(15);

#[derive(Serialize, Deserialize, ToSchema, Clone, Debug, PartialEq, Eq)]
pub enum ChargerStatus {
    Disconnected = 0,
    Connected = 1,
}

#[derive(Serialize, Deserialize, ToSchema, Clone)]
pub struct GetChargerSchema {
    pub(crate) id: String,
    pub(crate) uid: i32,
    pub(crate) name: String,
    pub(crate) note: Option<String>,
    pub(crate) status: ChargerStatus,
    pub(crate) port: i32,
    pub(crate) valid: bool,
    pub(crate) last_state_change: Option<i64>,
    pub(crate) firmware_version: String,
    pub(crate) added_at: Option<i64>,
}

#[derive(Serialize, Clone)]
#[serde(tag = "type")]
pub enum StateUpdateMessage {
    #[serde(rename = "state_change")]
    StateChange { charger: GetChargerSchema },
    #[serde(rename = "device_removed")]
    DeviceRemoved { charger_id: String },
    #[serde(rename = "groupings_changed")]
    GroupingsChanged,
}

/// Each tab has its own bounded queue. Updates are queued before fetching the
/// initial list, and only the WebSocket task writes frames, so the list always
/// arrives first. A slow client is disconnected and reconnects with a fresh list.
#[derive(Default)]
pub struct StateUpdateClients {
    clients: futures_util::lock::Mutex<
        HashMap<uuid::Uuid, HashMap<uuid::Uuid, mpsc::Sender<StateUpdateMessage>>>,
    >,
}

impl StateUpdateClients {
    pub(crate) async fn subscribe(
        &self,
        user_id: uuid::Uuid,
    ) -> (uuid::Uuid, mpsc::Receiver<StateUpdateMessage>) {
        let connection_id = uuid::Uuid::new_v4();
        let (sender, receiver) = mpsc::channel(64);
        self.clients
            .lock()
            .await
            .entry(user_id)
            .or_default()
            .insert(connection_id, sender);
        (connection_id, receiver)
    }

    pub(crate) async fn unsubscribe(&self, user_id: uuid::Uuid, connection_id: uuid::Uuid) {
        let mut clients = self.clients.lock().await;
        if let Some(connections) = clients.get_mut(&user_id) {
            connections.remove(&connection_id);
            if connections.is_empty() {
                clients.remove(&user_id);
            }
        }
    }

    pub(crate) async fn send(&self, user_id: uuid::Uuid, message: StateUpdateMessage) {
        let mut clients = self.clients.lock().await;
        if let Some(connections) = clients.get_mut(&user_id) {
            connections.retain(|_, sender| sender.try_send(message.clone()).is_ok());
            if connections.is_empty() {
                clients.remove(&user_id);
            }
        }
    }

    pub(crate) async fn shrink_to_fit(&self) {
        let mut clients = self.clients.lock().await;
        for connections in clients.values_mut() {
            connections.shrink_to_fit();
        }
        clients.shrink_to_fit();
    }
}

pub async fn fetch_chargers(
    state: &web::Data<AppState>,
    uid: uuid::Uuid,
    bridge_state: &web::Data<BridgeState<'_>>,
) -> Result<Vec<GetChargerSchema>, actix_web::Error> {
    use db_connector::schema::allowed_users::dsl as allowed_users;
    use db_connector::schema::chargers::dsl as chargers;

    let user = get_user(state, uid).await?;

    // A single query keeps associations and chargers consistent if a device
    // is removed while a client is fetching its initial snapshot.
    let devices: Vec<(Charger, AllowedUser)> = {
        let mut conn = get_connection(state).await?;
        allowed_users::allowed_users
            .inner_join(chargers::chargers)
            .filter(allowed_users::user_id.eq(user.id))
            .select((Charger::as_select(), AllowedUser::as_select()))
            .load(&mut conn)
            .await
            .map_err(|err| {
                log::error!("Failed to load devices: {err}");
                Error::InternalError
            })?
    };

    let device_map = bridge_state.device_management_map_with_id.lock().await;
    let devices = devices
        .into_iter()
        .map(|(c, allowed_user)| {
            let status = if device_map.contains_key(&c.id) {
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
                added_at: allowed_user.added_at.map(|ts| ts.and_utc().timestamp()),
            }
        })
        .collect::<Vec<GetChargerSchema>>();

    Ok(devices)
}

/// WebSocket endpoint for get_devices with live state updates
#[utoipa::path(
    get,
    path = "/charger/get_devices",
    responses(
        (status = 101, description = "WebSocket connection established. Returns initial charger list and live state updates."),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    ),
    security(
        ("jwt" = [])
    )
)]
#[get("/get_devices")]
pub async fn get_devices(
    req: HttpRequest,
    stream: web::Payload,
    state: web::Data<AppState>,
    uid: crate::models::uuid::Uuid,
    bridge_state: web::Data<BridgeState<'static>>,
) -> Result<impl Responder, actix_web::Error> {
    handle_websocket(req, state, uid, bridge_state, stream).await
}

async fn handle_websocket(
    req: HttpRequest,
    state: web::Data<AppState>,
    uid: crate::models::uuid::Uuid,
    bridge_state: web::Data<BridgeState<'static>>,
    stream: web::Payload,
) -> Result<HttpResponse, actix_web::Error> {
    let (resp, mut session, ws_stream) = actix_ws::handle(&req, stream)?;
    let mut ws_stream = ws_stream
        .aggregate_continuations()
        .max_continuation_size(2_usize.pow(20));

    let user_id: uuid::Uuid = uid.into();

    let (connection_id, mut updates) = bridge_state.state_update_clients.subscribe(user_id).await;

    rt::spawn(async move {
        let mut last_heartbeat = Instant::now();
        let mut interval_timer = interval(HEARTBEAT_INTERVAL);

        let result = async {
            let chargers = fetch_chargers(&state, user_id, &bridge_state).await?;
            send_json(&mut session, &chargers).await?;

            loop {
                tokio::select! {
                    message = ws_stream.next() => match message {
                        Some(Ok(AggregatedMessage::Close(_))) | None => break,
                        Some(Ok(AggregatedMessage::Ping(msg))) => {
                            session.pong(&msg).await
                                .map_err(actix_web::error::ErrorInternalServerError)?;
                            last_heartbeat = Instant::now();
                        }
                        Some(Ok(AggregatedMessage::Pong(_))) => {
                            last_heartbeat = Instant::now();
                        }
                        Some(Ok(_)) => {}
                        Some(Err(err)) => {
                            return Err(actix_web::error::ErrorInternalServerError(err));
                        }
                    },
                    update = updates.recv() => {
                        let Some(update) = update else { break };
                        send_json(&mut session, &update).await?;
                    }
                    _ = interval_timer.tick() => {
                        if last_heartbeat.elapsed() > CLIENT_TIMEOUT {
                            log::debug!("get_devices WebSocket client quietly quit.");
                            break;
                        }
                        session.ping(b"").await
                            .map_err(actix_web::error::ErrorInternalServerError)?;
                    }
                }
            }
            Ok::<_, actix_web::Error>(())
        }
        .await;

        if let Err(err) = result {
            log::error!("get_devices WebSocket for user {user_id} failed: {err}");
        }
        bridge_state
            .state_update_clients
            .unsubscribe(user_id, connection_id)
            .await;
        let _ = tokio::time::timeout(Duration::from_secs(2), session.close(None)).await;
    });

    Ok(resp)
}

async fn send_json(
    session: &mut actix_ws::Session,
    value: &impl Serialize,
) -> actix_web::Result<()> {
    tokio::time::timeout(
        Duration::from_secs(2),
        session.text(serde_json::to_string(value)?),
    )
    .await
    .map_err(actix_web::error::ErrorInternalServerError)?
    .map_err(actix_web::error::ErrorInternalServerError)
}

#[cfg(test)]
mod tests {
    use actix_web::web;
    use base64::{prelude::BASE64_STANDARD, Engine};
    use db_connector::{models::users::User, test_connection_pool};
    use diesel::{ExpressionMethods, QueryDsl, SelectableHelper};
    use diesel_async::RunQueryDsl as _AsyncRunQueryDsl;
    use std::str::FromStr;

    use super::*;
    use crate::{
        routes::{
            charger::allow_user::UserAuth,
            user::tests::{random_positive_charger_id, TestUser},
        },
        tests::{create_test_bridge_state, create_test_state},
    };

    /// Helper function to get AppState and BridgeState for testing fetch_chargers directly
    fn get_test_state() -> (web::Data<AppState>, web::Data<BridgeState<'static>>) {
        let pool = test_connection_pool();
        let state = create_test_state(Some(pool.clone()));
        let bridge_state = create_test_bridge_state(Some(pool));
        (state, bridge_state)
    }

    /// Helper function to get the user UUID from the email address
    async fn get_user_uuid_from_email(email_addr: &str) -> uuid::Uuid {
        use db_connector::schema::users::dsl::*;

        let pool = test_connection_pool();
        let mut conn = pool.get().await.unwrap();
        let email_owned = email_addr.to_string();
        users
            .filter(email.eq(email_owned))
            .select(User::as_select())
            .get_result::<User>(&mut conn)
            .await
            .expect("Failed to find user")
            .id
    }

    #[actix_web::test]
    async fn state_updates_reach_all_tabs_and_cleanup_only_the_closed_connection() {
        let clients = StateUpdateClients::default();
        let user_id = uuid::Uuid::new_v4();
        let other_id = uuid::Uuid::new_v4();
        let (first_id, mut first) = clients.subscribe(user_id).await;
        let (_, mut second) = clients.subscribe(user_id).await;
        let (_, mut other) = clients.subscribe(other_id).await;
        let message = StateUpdateMessage::DeviceRemoved {
            charger_id: "device".into(),
        };

        clients.send(user_id, message.clone()).await;
        for receiver in [&mut first, &mut second] {
            assert!(
                matches!(receiver.try_recv().unwrap(), StateUpdateMessage::DeviceRemoved { charger_id } if charger_id == "device")
            );
        }
        assert!(other.try_recv().is_err());

        clients.unsubscribe(user_id, first_id).await;
        let (_, mut replacement) = clients.subscribe(user_id).await;
        // A late cleanup from the old connection cannot remove newer tabs.
        clients.unsubscribe(user_id, first_id).await;
        clients.send(user_id, message).await;
        assert!(first.recv().await.is_none());
        assert!(second.try_recv().is_ok());
        assert!(replacement.try_recv().is_ok());
    }

    #[actix_web::test]
    async fn slow_state_update_client_is_disconnected_instead_of_losing_updates_silently() {
        let clients = StateUpdateClients::default();
        let user_id = uuid::Uuid::new_v4();
        let (_, mut slow) = clients.subscribe(user_id).await;
        let (_, mut fast) = clients.subscribe(user_id).await;
        for i in 0..65 {
            clients
                .send(
                    user_id,
                    StateUpdateMessage::DeviceRemoved {
                        charger_id: i.to_string(),
                    },
                )
                .await;
            assert!(fast.try_recv().is_ok());
        }
        for _ in 0..64 {
            assert!(slow.try_recv().is_ok());
        }
        assert!(slow.recv().await.is_none());
        clients
            .send(
                user_id,
                StateUpdateMessage::DeviceRemoved {
                    charger_id: "next".into(),
                },
            )
            .await;
        assert!(fast.try_recv().is_ok());
    }

    #[actix_web::test]
    async fn websocket_sends_initial_snapshot_before_queued_updates() {
        use crate::{middleware::jwt::JwtMiddleware, tests::configure};
        use actix_web::{body::MessageBody, cookie::Cookie, test, App};

        let (mut user, _) = TestUser::random().await;
        user.login().await;
        let user_id = get_user_uuid_from_email(&user.mail).await;
        let (_, bridge_state) = get_test_state();
        let app = test::init_service(
            App::new()
                .configure(configure)
                .app_data(bridge_state.clone())
                .wrap(JwtMiddleware)
                .service(get_devices),
        )
        .await;
        let mut req = test::TestRequest::get()
            .uri("/get_devices")
            .cookie(Cookie::new("access_token", user.get_access_token()))
            .append_header(("connection", "upgrade"))
            .append_header(("upgrade", "websocket"))
            .append_header(("sec-websocket-version", "13"))
            .append_header(("sec-websocket-key", "dGhlIHNhbXBsZSBub25jZQ=="))
            .to_request();
        // Keep the incoming stream open while inspecting outgoing WS frames.
        *req.payload() = actix_web::dev::Payload::Stream {
            payload: Box::pin(futures_util::stream::pending()),
        };
        let response = test::call_service(&app, req).await;
        assert_eq!(response.status(), 101);
        bridge_state
            .state_update_clients
            .send(
                user_id,
                StateUpdateMessage::DeviceRemoved {
                    charger_id: "removed".into(),
                },
            )
            .await;

        let mut body = Box::pin(response.into_body());
        let mut bytes = Vec::new();
        let mut text_frames = Vec::new();
        tokio::time::timeout(Duration::from_secs(2), async {
            while text_frames.len() < 2 {
                let chunk = futures_util::future::poll_fn(|cx| body.as_mut().poll_next(cx))
                    .await
                    .unwrap()
                    .unwrap();
                bytes.extend_from_slice(&chunk);
                while bytes.len() >= 2 {
                    // Both JSON payloads are short, unmasked server frames.
                    let length = bytes[1] as usize;
                    assert!(length < 126);
                    if bytes.len() < length + 2 {
                        break;
                    }
                    if bytes[0] == 0x81 {
                        text_frames.push(
                            serde_json::from_slice::<serde_json::Value>(&bytes[2..length + 2])
                                .unwrap(),
                        );
                    }
                    bytes.drain(..length + 2);
                }
            }
        })
        .await
        .expect("initial snapshot and queued update should be sent");
        assert_eq!(text_frames[0], serde_json::json!([]));
        assert_eq!(
            text_frames[1],
            serde_json::json!({"type": "device_removed", "charger_id": "removed"})
        );
        // Dropping the response closes the session; its failed heartbeat must
        // also clean up the subscription.
        drop(body);
        tokio::time::timeout(Duration::from_secs(7), async {
            while !bridge_state
                .state_update_clients
                .clients
                .lock()
                .await
                .is_empty()
            {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("closed subscription should be cleaned up");
    }

    #[actix_web::test]
    async fn live_device_updates_cover_sharing_notes_and_removal() {
        use crate::routes::charger::{
            allow_user::allow_user, remove::remove, update_note::update_note,
        };
        use crate::{middleware::jwt::JwtMiddleware, tests::configure};
        use actix_web::{cookie::Cookie, test, App};

        let (mut owner, _) = TestUser::random().await;
        let (mut guest, _) = TestUser::random().await;
        owner.login().await;
        guest.login().await;
        let device = owner.add_random_charger().await;
        let owner_id = get_user_uuid_from_email(&owner.mail).await;
        let guest_id = get_user_uuid_from_email(&guest.mail).await;
        let (_, bridge_state) = get_test_state();
        let (_, mut owner_tab) = bridge_state.state_update_clients.subscribe(owner_id).await;
        let (_, mut owner_other_tab) = bridge_state.state_update_clients.subscribe(owner_id).await;
        let (_, mut guest_tab) = bridge_state.state_update_clients.subscribe(guest_id).await;
        let (_, mut unrelated) = bridge_state
            .state_update_clients
            .subscribe(uuid::Uuid::new_v4())
            .await;

        let app = test::init_service(
            App::new()
                .configure(configure)
                .app_data(bridge_state.clone())
                .wrap(JwtMiddleware)
                .service(allow_user)
                .service(update_note)
                .service(remove),
        )
        .await;
        let req = test::TestRequest::put()
            .uri("/allow_user")
            .cookie(Cookie::new("access_token", owner.get_access_token()))
            .append_header(("X-Forwarded-For", "123.123.123.3"))
            .set_json(serde_json::json!({
                "charger_id": device.uuid,
                "charger_password": device.password,
                "email": guest.mail,
                "user_auth": {"LoginKey": BASE64_STANDARD.encode(guest.get_login_key().await)},
                "wg_keys": crate::routes::charger::add::tests::generate_random_keys(),
                "charger_name": "guest-name",
                "note": "guest-note",
            }))
            .to_request();
        assert!(test::call_service(&app, req).await.status().is_success());
        for receiver in [&mut owner_tab, &mut owner_other_tab, &mut guest_tab] {
            assert!(
                matches!(receiver.try_recv().unwrap(), StateUpdateMessage::StateChange { charger } if charger.id == device.uuid)
            );
        }

        let req = test::TestRequest::post()
            .uri("/update_note")
            .cookie(Cookie::new("access_token", owner.get_access_token()))
            .set_json(serde_json::json!({"charger_id": device.uuid, "note": "owner-note"}))
            .to_request();
        assert!(test::call_service(&app, req).await.status().is_success());
        for receiver in [&mut owner_tab, &mut owner_other_tab] {
            assert!(
                matches!(receiver.try_recv().unwrap(), StateUpdateMessage::StateChange { charger } if charger.note.as_deref() == Some("owner-note"))
            );
        }
        assert!(
            matches!(guest_tab.try_recv().unwrap(), StateUpdateMessage::StateChange { charger } if charger.note.as_deref() == Some("guest-note") && charger.name == "guest-name")
        );

        // Removing a shared device only removes it for the requesting user.
        let req = test::TestRequest::delete()
            .uri("/remove")
            .cookie(Cookie::new("access_token", owner.get_access_token()))
            .set_json(serde_json::json!({"charger": device.uuid}))
            .to_request();
        assert!(test::call_service(&app, req).await.status().is_success());
        for receiver in [&mut owner_tab, &mut owner_other_tab] {
            assert!(
                matches!(receiver.try_recv().unwrap(), StateUpdateMessage::DeviceRemoved { charger_id } if charger_id == device.uuid)
            );
        }
        assert!(guest_tab.try_recv().is_err());

        // The final user receives a removal even after the charger row is gone.
        let req = test::TestRequest::delete()
            .uri("/remove")
            .cookie(Cookie::new("access_token", guest.get_access_token()))
            .set_json(serde_json::json!({"charger": device.uuid}))
            .to_request();
        assert!(test::call_service(&app, req).await.status().is_success());
        assert!(
            matches!(guest_tab.try_recv().unwrap(), StateUpdateMessage::DeviceRemoved { charger_id } if charger_id == device.uuid)
        );
        assert!(unrelated.try_recv().is_err());
    }

    #[actix_web::test]
    async fn registration_notifies_existing_device_list_subscriptions() {
        use crate::routes::charger::{
            add::{add, encode_charger_uid, tests::generate_random_keys},
            add_with_token::add_with_token,
        };
        use crate::{middleware::jwt::JwtMiddleware, tests::configure};
        use actix_web::{cookie::Cookie, test, App};

        let (mut user, _) = TestUser::random().await;
        user.login().await;
        let user_id = get_user_uuid_from_email(&user.mail).await;
        let token = user.create_authorization_token(true).await;
        let (_, bridge_state) = get_test_state();
        let (_, mut updates) = bridge_state.state_update_clients.subscribe(user_id).await;
        let app = test::init_service(
            App::new()
                .configure(configure)
                .app_data(bridge_state)
                .wrap(JwtMiddleware)
                .service(add)
                .service(add_with_token),
        )
        .await;

        for with_token in [false, true] {
            let keys = generate_random_keys();
            let uid = random_positive_charger_id();
            let mut body = serde_json::json!({
                "charger": {
                    "uid": encode_charger_uid(uid), "charger_pub": keys[0].charger_public,
                    "wg_charger_ip": "0.0.0.0/0", "wg_server_ip": "0.0.0.0/0", "psk": "",
                },
                "keys": keys, "name": "name", "note": "note",
            });
            let path = if with_token {
                body["token"] = token.token.clone().into();
                body["user_id"] = user_id.to_string().into();
                "/add_with_token"
            } else {
                "/add"
            };
            let req = test::TestRequest::put()
                .uri(path)
                .cookie(Cookie::new("access_token", user.get_access_token()))
                .set_json(body)
                .to_request();
            let response = test::call_service(&app, req).await;
            assert!(response.status().is_success());
            let response: crate::routes::charger::add::AddChargerResponseSchema =
                test::read_body_json(response).await;
            assert!(
                matches!(updates.try_recv().unwrap(), StateUpdateMessage::StateChange { charger } if charger.id == response.charger_uuid && charger.uid == uid && charger.note.as_deref() == Some("note"))
            );
            crate::routes::charger::remove::tests::remove_test_device(&response.charger_uuid).await;
        }
    }

    /// Test if only the chargers the user has access to will be returned.
    #[actix_web::test]
    async fn test_get_devices() {
        let (mut user1, _) = TestUser::random().await;
        let (mut user2, _) = TestUser::random().await;
        user1.login().await;
        user2.login().await;
        for _ in 0..5 {
            let _ = user1.add_random_charger().await;
            let device = user2.add_random_charger().await;
            user2
                .allow_user(
                    &user1.mail,
                    UserAuth::LoginKey(BASE64_STANDARD.encode(user1.get_login_key().await)),
                    &device,
                )
                .await;
        }
        for _ in 0..5 {
            let uuid = random_positive_charger_id();
            user2.add_charger(uuid).await;
        }

        let (state, bridge_state) = get_test_state();
        let user_id = get_user_uuid_from_email(&user1.mail).await;

        let resp = fetch_chargers(&state, user_id, &bridge_state)
            .await
            .expect("fetch_chargers failed");
        assert_eq!(resp.len(), 10);
    }

    #[actix_web::test]
    async fn test_shared_device_has_account_specific_added_time() {
        use db_connector::schema::allowed_users::dsl as allowed_users;

        let (mut owner, _) = TestUser::random().await;
        let (mut guest, _) = TestUser::random().await;
        owner.login().await;
        guest.login().await;
        let device = owner.add_random_charger().await;
        owner
            .allow_user(
                &guest.mail,
                UserAuth::LoginKey(BASE64_STANDARD.encode(guest.get_login_key().await)),
                &device,
            )
            .await;

        let owner_id = get_user_uuid_from_email(&owner.mail).await;
        let guest_id = get_user_uuid_from_email(&guest.mail).await;
        let charger_id = uuid::Uuid::from_str(&device.uuid).unwrap();
        let owner_added = chrono::DateTime::from_timestamp(1_700_000_000, 0)
            .unwrap()
            .naive_utc();
        let guest_added = chrono::DateTime::from_timestamp(1_710_000_000, 0)
            .unwrap()
            .naive_utc();
        let pool = test_connection_pool();
        let mut conn = pool.get().await.unwrap();
        for (uid, added) in [(owner_id, owner_added), (guest_id, guest_added)] {
            diesel::update(
                allowed_users::allowed_users
                    .filter(allowed_users::charger_id.eq(charger_id))
                    .filter(allowed_users::user_id.eq(uid)),
            )
            .set(allowed_users::added_at.eq(Some(added)))
            .execute(&mut conn)
            .await
            .unwrap();
        }
        drop(conn);

        let (state, bridge_state) = get_test_state();
        let owner_devices = fetch_chargers(&state, owner_id, &bridge_state)
            .await
            .unwrap();
        let guest_devices = fetch_chargers(&state, guest_id, &bridge_state)
            .await
            .unwrap();
        assert_eq!(owner_devices[0].added_at, Some(1_700_000_000));
        assert_eq!(guest_devices[0].added_at, Some(1_710_000_000));
    }

    #[actix_web::test]
    async fn test_get_not_existing_chargers() {
        let (mut user1, _) = TestUser::random().await;
        user1.login().await;

        let (state, bridge_state) = get_test_state();
        let user_id = get_user_uuid_from_email(&user1.mail).await;

        let resp = fetch_chargers(&state, user_id, &bridge_state)
            .await
            .expect("fetch_chargers failed");
        assert_eq!(resp.len(), 0);
    }

    /// A device whose association was removed must be absent from the snapshot.
    #[actix_web::test]
    async fn test_deleted_association_is_absent_from_initial_snapshot() {
        let (mut user1, _) = TestUser::random().await;
        user1.login().await;

        // Add two chargers - we'll delete the allowed_user entry for one of them
        let device1 = user1.add_random_charger().await;
        let device2 = user1.add_random_charger().await;

        // Remove only the association, leaving the charger row in place.
        {
            use db_connector::schema::allowed_users::dsl::*;

            let device1_uuid = uuid::Uuid::from_str(&device1.uuid).unwrap();
            let pool = test_connection_pool();
            let mut conn = pool.get().await.unwrap();
            diesel::delete(allowed_users.filter(charger_id.eq(device1_uuid)))
                .execute(&mut conn)
                .await
                .expect("Failed to delete allowed_user entry");
        }

        let (state, bridge_state) = get_test_state();
        let user_id = get_user_uuid_from_email(&user1.mail).await;

        // This should not panic even though device1 has no allowed_users entry
        let resp = fetch_chargers(&state, user_id, &bridge_state)
            .await
            .expect("fetch_chargers failed");

        // Only device2 should be returned since device1's allowed_user was deleted
        assert_eq!(resp.len(), 1);
        assert_eq!(resp[0].id, device2.uuid);
    }
}
