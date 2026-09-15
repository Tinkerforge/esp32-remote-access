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
        use diesel::r2d2::ConnectionManager;
        use diesel::PgConnection;

        let manager = ConnectionManager::<PgConnection>::new("postgres://nobody@localhost/nobody");
        let pool = Pool::builder().max_size(1).build_unchecked(manager);

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
