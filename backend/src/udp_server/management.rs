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

use std::net::SocketAddr;

use actix_web::web;
use anyhow::Error;
use serde::{ser::SerializeStruct, Serialize};

use crate::BridgeState;

use super::packet::{
    ManagementPacket, ManagementPacketHeader, ManagementResponsePacket, ManagementResponseV2,
    OldManagementResponse, PacketType, RemoveUserCommand, RemoveUserCommandPacket,
};

/// Reattach saved relay sessions and ask browsers to rebuild their WireGuard
/// client. The previous client's IPv6 peer and TCP streams are stale after a
/// device restart. Other relay clients can continue using the reopened route.
pub(crate) async fn reconnect_lost_clients(
    state: &web::Data<BridgeState<'_>>,
    charger_id: uuid::Uuid,
) {
    let socket = state
        .device_management_map_with_id
        .lock()
        .await
        .get(&charger_id)
        .cloned();
    let Some(socket) = socket else { return };
    let clients = state.lost_connections.lock().await.remove(&charger_id);
    if let Some(clients) = clients {
        for (conn_no, mut session) in clients {
            state.undiscovered_clients.lock().await.insert(
                RemoteConnMeta {
                    charger_id,
                    conn_no,
                },
                session.clone(),
            );
            crate::ws_udp_bridge::open_connection(
                conn_no,
                charger_id,
                socket.clone(),
                state.port_discovery.clone(),
            )
            .await
            .ok();
            let _ =
                tokio::time::timeout(std::time::Duration::from_secs(2), session.text("reconnect"))
                    .await;
        }
    }
}

#[derive(PartialEq, Hash, Eq, Debug, Clone)]
pub struct RemoteConnMeta {
    pub charger_id: uuid::Uuid,
    pub conn_no: i32,
}

impl Serialize for RemoteConnMeta {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut s = serializer.serialize_struct("RemoteConnMeta", 2)?;
        s.serialize_field("charger_id", &self.charger_id.to_string())?;
        s.serialize_field("conn_no", &self.conn_no)?;
        s.end()
    }
}

/// Save browser sessions on identification and wake them once a management
/// tunnel exists. HTTPS identification can arrive before or after its handshake.
pub(crate) async fn reconnect_user_sessions(
    state: &web::Data<BridgeState<'_>>,
    charger_id: uuid::Uuid,
) {
    let addresses = {
        let mut routes = state.device_remote_conn_map.lock().await;
        let mut addresses = Vec::new();
        routes.retain(|meta, addr| {
            if meta.charger_id == charger_id {
                addresses.push((*addr, meta.conn_no));
                false
            } else {
                true
            }
        });
        addresses
    };
    let mut sessions = {
        let mut clients = state.web_client_map.lock().await;
        addresses
            .into_iter()
            .filter_map(|(addr, conn_no)| clients.remove(&addr).map(|session| (conn_no, session)))
            .collect::<Vec<_>>()
    };
    // A second restart can arrive while a browser is still waiting for port
    // discovery. Those clients also need a fresh tunnel once the device is up.
    {
        let mut clients = state.undiscovered_clients.lock().await;
        clients.retain(|meta, session| {
            if meta.charger_id == charger_id {
                sessions.push((meta.conn_no, session.clone()));
                false
            } else {
                true
            }
        });
    }
    state
        .port_discovery
        .lock()
        .await
        .retain(|response, _| response.charger_id != charger_id.as_u128());
    if !sessions.is_empty() {
        state
            .lost_connections
            .lock()
            .await
            .entry(charger_id)
            .or_default()
            .extend(sessions);
    }
    reconnect_lost_clients(state, charger_id).await;
}

async fn process_old_packet(
    state: &web::Data<BridgeState<'_>>,
    data: &[u8],
) -> anyhow::Result<ManagementResponseV2> {
    let packet: OldManagementResponse = unsafe { std::ptr::read(data.as_ptr() as *const _) };

    let map = state.port_discovery.lock().await;
    for meta in map.iter() {
        let meta = meta.0;
        if meta.connection_no == packet.connection_no
            && meta.connection_uuid == packet.connection_uuid
        {
            return Ok(*meta);
        }
    }

    Err(Error::msg("Unknown connection"))
}

async fn unpack_packet(
    state: &web::Data<BridgeState<'_>>,
    data: &[u8],
) -> anyhow::Result<ManagementResponseV2> {
    if data.len() == ::core::mem::size_of::<OldManagementResponse>() {
        process_old_packet(state, data).await
    } else if data.len() == ::core::mem::size_of::<ManagementResponsePacket>() {
        let packet: ManagementResponsePacket = unsafe { std::ptr::read(data.as_ptr() as *const _) };
        if packet.header.magic != 0x1234 || packet.header.version != 1 {
            return Err(Error::msg("Not a valid ManagementResponse packet"));
        }

        Ok(packet.data)
    } else {
        Err(Error::msg("Received a packet of invalid length"))
    }
}

pub async fn try_port_discovery(
    state: &web::Data<BridgeState<'_>>,
    data: &[u8],
    addr: SocketAddr,
) -> anyhow::Result<()> {
    let response = unpack_packet(state, data).await?;

    {
        let mut set = state.port_discovery.lock().await;
        if set.remove(&response).is_none() {
            return Err(Error::msg("Connection does not exist"));
        }
    }

    let meta = RemoteConnMeta {
        charger_id: uuid::Uuid::from_u128(response.charger_id),
        conn_no: response.connection_no,
    };

    {
        let mut map = state.undiscovered_clients.lock().await;
        if let Some(r) = map.remove(&meta) {
            let mut map = state.web_client_map.lock().await;
            map.insert(addr, r);
        }
    }

    let mut map = state.device_remote_conn_map.lock().await;
    map.insert(meta, addr);

    Ok(())
}

/// Snapshot candidates without retaining map locks across a socket wait. A
/// response or cleanup may invalidate the snapshot; recheck after acquiring
/// the socket and keep discovery locked through the synchronous send.
pub async fn resend_pending_connections(state: &web::Data<BridgeState<'_>>) {
    use super::packet::{ManagementCommand, ManagementCommandId, ManagementCommandPacket};
    let pending: Vec<_> = state.port_discovery.lock().await.keys().copied().collect();
    for response in pending {
        let id = uuid::Uuid::from_u128(response.charger_id);
        let socket = state
            .device_management_map_with_id
            .lock()
            .await
            .get(&id)
            .cloned();
        let Some(socket_handle) = socket else {
            continue;
        };
        let mut socket = socket_handle.lock().await;
        let current = state.device_management_map_with_id.lock().await;
        if !current
            .get(&id)
            .is_some_and(|entry| std::sync::Arc::ptr_eq(entry, &socket_handle))
        {
            continue;
        }
        let discovery = state.port_discovery.lock().await;
        if !discovery.contains_key(&response) {
            continue;
        }
        let command = ManagementCommand {
            command_id: ManagementCommandId::Connect,
            connection_no: response.connection_no,
            connection_uuid: response.connection_uuid,
        };
        let header = ManagementPacketHeader {
            magic: 0x1234,
            length: std::mem::size_of::<ManagementCommand>() as u16,
            seq_number: 0,
            version: 1,
            p_type: PacketType::ManagementCommand,
        };
        socket.send_packet(ManagementPacket::CommandPacket(ManagementCommandPacket {
            header,
            command,
        }));
    }
}

/// Prompt a connected charger to drop a specific user from its configured
/// users list.
///
/// Called by the server-side cleanup paths that just removed a user from a
/// charger's `allowed_users` (e.g. `DELETE /charger/remove` or
/// `DELETE /user/delete`). If the charger is currently connected over its
/// WireGuard management channel the command is delivered right away;
/// otherwise it is silently dropped and the charger will catch up the next
/// time it calls `PUT /management`.
pub async fn prompt_charger_to_remove_user(
    bridge_state: &web::Data<BridgeState<'_>>,
    charger_id: uuid::Uuid,
    user_id: uuid::Uuid,
) {
    let socket = {
        let map = bridge_state.device_management_map_with_id.lock().await;
        map.get(&charger_id).cloned()
    };

    let Some(socket) = socket else {
        // The charger is offline; the existing `PUT /management` flow will
        // reconcile the configured users list once it comes back online.
        return;
    };

    let command = RemoveUserCommand {
        user_uuid: user_id.as_u128(),
    };

    let header = ManagementPacketHeader {
        magic: 0x1234,
        length: std::mem::size_of::<RemoveUserCommand>() as u16,
        seq_number: 0,
        version: 1,
        p_type: PacketType::RemoveUser,
    };

    let packet = RemoveUserCommandPacket { header, command };

    let mut sock = socket.lock().await;
    sock.send_packet(ManagementPacket::RemoveUserPacket(packet));
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Arc;

    use actix_web::web;
    use futures_util::lock::Mutex;

    use super::super::socket::ManagementSocket;
    use super::*;

    /// Drain the captured plaintext packets from a packet-capture sink.
    fn take_captured(capture: &std::sync::Mutex<Vec<Vec<u8>>>) -> Vec<Vec<u8>> {
        capture.lock().unwrap().drain(..).collect()
    }

    /// Parse a captured plaintext packet and extract the user uuid it carries.
    /// Returns `None` if the bytes don't look like a valid `RemoveUser`
    /// packet.
    ///
    /// `ManagementSocket::send_packet` records the bytes produced by
    /// `ManagementPacket::as_bytes()`, which strips the discriminant byte of
    /// the `ManagementPacket` enum but keeps any trailing padding bytes the
    /// compiler inserts between variants. We only look at the leading
    /// `RemoveUserCommandPacket`-sized chunk of the captured buffer.
    fn extract_remove_user_user_uuid(bytes: &[u8]) -> Option<u128> {
        let expected = std::mem::size_of::<RemoveUserCommandPacket>();
        if bytes.len() < expected {
            return None;
        }
        // `RemoveUserCommandPacket` is `#[repr(C, packed)]` so we have to
        // use `read_unaligned` instead of taking references to its fields.
        let packet =
            unsafe { std::ptr::read_unaligned(bytes.as_ptr() as *const RemoveUserCommandPacket) };
        let magic = { packet.header.magic };
        let version = { packet.header.version };
        let p_type = { packet.header.p_type };
        let length = { packet.header.length };
        let user_uuid = { packet.command.user_uuid };

        if magic != 0x1234 || version != 1 {
            return None;
        }
        if p_type != PacketType::RemoveUser {
            return None;
        }
        if length as usize != std::mem::size_of::<RemoveUserCommand>() {
            return None;
        }
        Some(user_uuid)
    }

    /// Insert a fake management socket into the bridge state and attach a
    /// plaintext packet-capture sink so the test can observe what is sent.
    async fn install_capturing_socket(
        bridge_state: &web::Data<crate::BridgeState<'static>>,
        charger_id: uuid::Uuid,
    ) -> Arc<std::sync::Mutex<Vec<Vec<u8>>>> {
        let remote: std::net::SocketAddr = "127.0.0.1:51820".parse().unwrap();
        let mut socket = ManagementSocket::new_for_test(charger_id, remote).await;
        let capture = Arc::new(std::sync::Mutex::new(Vec::<Vec<u8>>::new()));
        socket.enable_packet_capture(capture.clone());
        let socket = Arc::new(Mutex::new(socket));

        let mut map = bridge_state.device_management_map_with_id.lock().await;
        map.insert(charger_id, socket);
        capture
    }

    /// Build a `BridgeState` for tests that don't actually talk to the
    /// database. `create_test_bridge_state` always builds a real DB pool,
    /// which makes it unsuitable for unit tests that only exercise the
    /// management-channel packet flow. This helper uses `build_unchecked`
    /// so the pool is never opened until something actually calls `.get()`
    /// on it.
    fn bridge_state_without_db() -> web::Data<crate::BridgeState<'static>> {
        use db_connector::Pool;
        use diesel_async::pooled_connection::AsyncDieselConnectionManager;
        use diesel_async::AsyncPgConnection;

        // A bogus connection URL so the manager never tries to actually
        // connect; the pool only opens a real connection on `pool.get()`,
        // which these tests never call.
        let manager = AsyncDieselConnectionManager::<AsyncPgConnection>::new(
            "postgres://nobody@localhost/nobody",
        );
        let pool: Pool = Pool::builder(manager)
            .max_size(1)
            .build()
            .expect("build dummy pool");

        let std_socket = std::net::UdpSocket::bind("0.0.0.0:0").unwrap();
        std_socket.set_nonblocking(true).unwrap();
        let bridge_state = crate::BridgeState {
            pool,
            device_management_map: Arc::new(Mutex::new(HashMap::new())),
            device_management_map_with_id: Arc::new(Mutex::new(HashMap::new())),
            port_discovery: Arc::new(Mutex::new(HashMap::new())),
            device_remote_conn_map: Mutex::new(HashMap::new()),
            undiscovered_clients: Mutex::new(HashMap::new()),
            web_client_map: Mutex::new(HashMap::new()),
            undiscovered_devices: Arc::new(Mutex::new(HashMap::new())),
            lost_connections: Mutex::new(HashMap::new()),
            socket: Arc::new(tokio::net::UdpSocket::from_std(std_socket).unwrap()),
            state_update_clients: Mutex::new(HashMap::new()),
            device_ratelimiter: Arc::new(crate::rate_limit::ChargerRateLimiter::new()),
        };

        web::Data::new(bridge_state)
    }

    fn browser_session() -> (actix_ws::Session, actix_web::HttpResponse) {
        use actix_web::FromRequest;
        use futures_util::FutureExt;
        let (request, mut payload) = actix_web::test::TestRequest::get()
            .insert_header(("Upgrade", "websocket"))
            .insert_header(("Connection", "upgrade"))
            .insert_header(("Sec-WebSocket-Version", "13"))
            .insert_header(("Sec-WebSocket-Key", "dGhlIHNhbXBsZSBub25jZQ=="))
            .to_http_parts();
        let payload = web::Payload::from_request(&request, &mut payload)
            .now_or_never()
            .unwrap()
            .unwrap();
        let (response, session, _) = actix_ws::handle(&request, payload).unwrap();
        (session, response)
    }

    #[actix_web::test]
    async fn reconnect_rebuilds_active_and_pending_browsers_when_tunnel_already_exists() {
        let state = bridge_state_without_db();
        let id = uuid::Uuid::new_v4();
        let other_id = uuid::Uuid::new_v4();
        let capture = install_capturing_socket(&state, id).await;
        let (active, active_response) = browser_session();
        let (pending, pending_response) = browser_session();
        let (mut other, _other_response) = browser_session();
        let address = "127.0.0.1:51821".parse().unwrap();
        let meta = RemoteConnMeta {
            charger_id: id,
            conn_no: 1,
        };
        state
            .device_remote_conn_map
            .lock()
            .await
            .insert(meta.clone(), address);
        state
            .web_client_map
            .lock()
            .await
            .insert(address, active.clone());
        state.undiscovered_clients.lock().await.insert(
            RemoteConnMeta {
                charger_id: id,
                conn_no: 2,
            },
            pending.clone(),
        );
        let other_meta = RemoteConnMeta {
            charger_id: other_id,
            conn_no: 1,
        };
        state
            .undiscovered_clients
            .lock()
            .await
            .insert(other_meta.clone(), other.clone());
        let command = ManagementResponseV2 {
            charger_id: id.as_u128(),
            connection_no: 2,
            connection_uuid: 1,
        };
        let other_command = ManagementResponseV2 {
            charger_id: other_id.as_u128(),
            connection_no: 1,
            connection_uuid: 2,
        };
        state.port_discovery.lock().await.extend([
            (command, std::time::Instant::now()),
            (other_command, std::time::Instant::now()),
        ]);

        reconnect_user_sessions(&state, id).await;

        assert!(other.ping(b"").await.is_ok());
        assert!(!state
            .device_remote_conn_map
            .lock()
            .await
            .contains_key(&meta));
        assert!(state.web_client_map.lock().await.is_empty());
        assert_eq!(state.undiscovered_clients.lock().await.len(), 3);
        assert!(!state.lost_connections.lock().await.contains_key(&id));
        let discovery = state.port_discovery.lock().await;
        assert!(!discovery.contains_key(&command));
        assert!(discovery.contains_key(&other_command));
        // Browsers receive an explicit reset request. CLI relay clients can
        // ignore the text message and keep using their reopened route.
        active.close(None).await.unwrap();
        pending.close(None).await.unwrap();
        state
            .undiscovered_clients
            .lock()
            .await
            .retain(|meta, _| meta.charger_id != id);
        for response in [active_response, pending_response] {
            assert_eq!(
                actix_web::body::to_bytes(response.into_body())
                    .await
                    .unwrap(),
                &b"\x81\x09reconnect\x88\x00"[..]
            );
        }
        assert_eq!(take_captured(&capture).len(), 2);
    }

    #[actix_web::test]
    async fn reconnect_waits_for_tunnel_and_preserves_browsers_across_repeated_identification() {
        let state = bridge_state_without_db();
        let id = uuid::Uuid::new_v4();
        let (mut first, first_response) = browser_session();
        let (second, second_response) = browser_session();
        let (mut unrelated, _unrelated_response) = browser_session();
        let other_id = uuid::Uuid::new_v4();
        state
            .lost_connections
            .lock()
            .await
            .insert(other_id, vec![(1, unrelated.clone())]);
        state.undiscovered_clients.lock().await.insert(
            RemoteConnMeta {
                charger_id: id,
                conn_no: 1,
            },
            first.clone(),
        );
        reconnect_user_sessions(&state, id).await;
        assert!(
            first.ping(b"").await.is_ok(),
            "browser woke before a tunnel existed"
        );
        state.undiscovered_clients.lock().await.insert(
            RemoteConnMeta {
                charger_id: id,
                conn_no: 2,
            },
            second.clone(),
        );
        reconnect_user_sessions(&state, id).await;
        assert_eq!(state.lost_connections.lock().await[&id].len(), 2);

        // This is also called by the UDP path after publishing a new tunnel.
        install_capturing_socket(&state, id).await;
        reconnect_lost_clients(&state, id).await;
        first.close(None).await.unwrap();
        second.close(None).await.unwrap();
        state
            .undiscovered_clients
            .lock()
            .await
            .retain(|meta, _| meta.charger_id != id);
        for response in [first_response, second_response] {
            let frames = actix_web::body::to_bytes(response.into_body())
                .await
                .unwrap();
            assert!(frames.windows(9).any(|message| message == b"reconnect"));
        }
        assert!(unrelated.ping(b"").await.is_ok());
        assert!(!state.lost_connections.lock().await.contains_key(&id));
        assert!(state.lost_connections.lock().await.contains_key(&other_id));
    }

    #[actix_web::test]
    async fn stopping_browser_preserves_other_lost_clients_and_reserves_key_until_cleanup() {
        use crate::{ws_udp_bridge::WebClient, AppState};
        use std::{collections::HashSet, future::Future, task::Context, time::Instant};
        let state = bridge_state_without_db();
        let id = uuid::Uuid::new_v4();
        let key = uuid::Uuid::new_v4();
        let app = web::Data::new(AppState {
            pool: state.pool.clone(),
            jwt_secret: String::new(),
            mailer: None,
            frontend_url: String::new(),
            sender_email: String::new(),
            sender_name: String::new(),
            brand: Default::default(),
            keys_in_use: Mutex::new(HashSet::from([key])),
            hasher: Default::default(),
        });
        let (session, _response) = browser_session();
        let (mut other, _other_response) = browser_session();
        let client = WebClient::new(key, id, app.clone(), state.clone(), 1, session.clone()).await;
        state
            .lost_connections
            .lock()
            .await
            .insert(id, vec![(1, session), (2, other.clone())]);
        let command = ManagementResponseV2 {
            charger_id: id.as_u128(),
            connection_no: 1,
            connection_uuid: 1,
        };
        let other_command = ManagementResponseV2 {
            charger_id: id.as_u128(),
            connection_no: 2,
            connection_uuid: 2,
        };
        state
            .port_discovery
            .lock()
            .await
            .extend([(command, Instant::now()), (other_command, Instant::now())]);
        install_capturing_socket(&state, id).await;
        let socket = state.device_management_map_with_id.lock().await[&id].clone();
        let guard = socket.lock().await;
        let mut stop = Box::pin(client.stop());
        let mut cx = Context::from_waker(futures_util::task::noop_waker_ref());
        assert!(stop.as_mut().poll(&mut cx).is_pending());
        assert!(
            app.keys_in_use.lock().await.contains(&key),
            "released key before old Disconnect was sent"
        );
        drop(guard);
        stop.await;

        assert!(!app.keys_in_use.lock().await.contains(&key));
        assert_eq!(state.lost_connections.lock().await[&id].len(), 1);
        assert_eq!(state.lost_connections.lock().await[&id][0].0, 2);
        assert!(other.ping(b"").await.is_ok());
        let discovery = state.port_discovery.lock().await;
        assert!(!discovery.contains_key(&command));
        assert!(discovery.contains_key(&other_command));
    }

    #[actix_web::test]
    async fn resend_rechecks_discovery_after_waiting_for_socket() {
        use std::{
            future::Future,
            task::Context,
            time::{Duration, Instant},
        };
        let state = bridge_state_without_db();
        let id = uuid::Uuid::new_v4();
        let capture = install_capturing_socket(&state, id).await;
        let socket = state
            .device_management_map_with_id
            .lock()
            .await
            .get(&id)
            .cloned()
            .unwrap();
        let pending = ManagementResponseV2 {
            charger_id: id.as_u128(),
            connection_no: 1,
            connection_uuid: 123,
        };
        state
            .port_discovery
            .lock()
            .await
            .insert(pending, Instant::now());
        let guard = socket.lock().await;
        let mut resend = Box::pin(resend_pending_connections(&state));
        let mut cx = Context::from_waker(futures_util::task::noop_waker_ref());
        assert!(resend.as_mut().poll(&mut cx).is_pending());
        // Simulate receipt of a discovery response while resend is suspended.
        state
            .port_discovery
            .try_lock()
            .expect("resend retained discovery lock")
            .remove(&pending);
        drop(guard);
        tokio::time::timeout(Duration::from_secs(1), resend)
            .await
            .unwrap();
        assert!(
            take_captured(&capture).is_empty(),
            "resent a completed command"
        );
        state
            .port_discovery
            .lock()
            .await
            .insert(pending, Instant::now());
        resend_pending_connections(&state).await;
        assert_eq!(take_captured(&capture).len(), 1);
    }

    #[actix_web::test]
    async fn resend_does_not_use_a_socket_replaced_during_its_wait() {
        use std::{future::Future, task::Context, time::Instant};
        let state = bridge_state_without_db();
        let id = uuid::Uuid::new_v4();
        let capture = install_capturing_socket(&state, id).await;
        let socket = state
            .device_management_map_with_id
            .lock()
            .await
            .get(&id)
            .cloned()
            .unwrap();
        let pending = ManagementResponseV2 {
            charger_id: id.as_u128(),
            connection_no: 1,
            connection_uuid: 123,
        };
        state
            .port_discovery
            .lock()
            .await
            .insert(pending, Instant::now());
        let guard = socket.lock().await;
        let mut resend = Box::pin(resend_pending_connections(&state));
        let mut cx = Context::from_waker(futures_util::task::noop_waker_ref());
        assert!(resend.as_mut().poll(&mut cx).is_pending());
        drop(guard);
        let addr = "127.0.0.1:2000".parse().unwrap();
        let replacement = ManagementSocket::new_for_test(id, addr).await;
        super::super::registry::publish(
            &state.device_management_map,
            &state.device_management_map_with_id,
            id,
            addr,
            replacement,
            1,
        )
        .await
        .unwrap();
        resend.await;
        assert!(
            take_captured(&capture).is_empty(),
            "resent through a superseded socket"
        );
    }

    #[actix_web::test]
    async fn prompt_charger_to_remove_user_sends_remove_user_packet() {
        let bridge_state = bridge_state_without_db();
        let charger_id = uuid::Uuid::new_v4();
        let user_id = uuid::Uuid::new_v4();
        let capture = install_capturing_socket(&bridge_state, charger_id).await;

        prompt_charger_to_remove_user(&bridge_state, charger_id, user_id).await;

        let packets = take_captured(&capture);
        assert_eq!(
            packets.len(),
            1,
            "expected exactly one packet to be sent, got {}",
            packets.len()
        );

        let packet = extract_remove_user_user_uuid(&packets[0])
            .expect("captured packet should be a valid RemoveUser packet");

        assert_eq!(packet, user_id.as_u128());
    }

    #[actix_web::test]
    async fn prompt_charger_to_remove_user_does_nothing_when_offline() {
        let bridge_state = bridge_state_without_db();
        let charger_id = uuid::Uuid::new_v4();
        let user_id = uuid::Uuid::new_v4();

        // No socket is inserted; the helper should be a no-op.
        prompt_charger_to_remove_user(&bridge_state, charger_id, user_id).await;

        // The map itself is empty so we have nothing to assert against.
        let map = bridge_state.device_management_map_with_id.lock().await;
        assert!(map.is_empty(), "bridge state should still be empty");
    }

    #[actix_web::test]
    async fn prompt_charger_to_remove_user_does_not_touch_other_chargers() {
        let bridge_state = bridge_state_without_db();

        let target_charger = uuid::Uuid::new_v4();
        let target_user = uuid::Uuid::new_v4();
        let target_capture = install_capturing_socket(&bridge_state, target_charger).await;

        let other_charger = uuid::Uuid::new_v4();
        let other_capture = install_capturing_socket(&bridge_state, other_charger).await;

        prompt_charger_to_remove_user(&bridge_state, target_charger, target_user).await;

        assert_eq!(take_captured(&target_capture).len(), 1);
        assert!(
            take_captured(&other_capture).is_empty(),
            "unrelated charger must not receive any packet",
        );
    }
}
