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

use super::packet::{ManagementResponsePacket, ManagementResponseV2, OldManagementResponse};

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

fn process_old_packet(
    state: &web::Data<BridgeState>,
    data: &[u8],
) -> anyhow::Result<ManagementResponseV2> {
    let packet: OldManagementResponse = unsafe { std::ptr::read(data.as_ptr() as *const _) };

    let map = state.port_discovery.lock().unwrap();
    for (meta, _) in map.iter() {
        if meta.connection_no == packet.connection_no
            && meta.connection_uuid == packet.connection_uuid
        {
            return Ok(meta.clone());
        }
    }

    Err(Error::msg("Unknown connection"))
}

fn unpack_packet(
    state: &web::Data<BridgeState>,
    data: &[u8],
) -> anyhow::Result<ManagementResponseV2> {
    if data.len() == ::core::mem::size_of::<OldManagementResponse>() {
        process_old_packet(state, data)
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

pub fn try_port_discovery(
    state: &web::Data<BridgeState>,
    data: &[u8],
    addr: SocketAddr,
) -> anyhow::Result<()> {
    let response = unpack_packet(state, data)?;

    {
        let mut set = state.port_discovery.lock().unwrap();
        if set.remove(&response).is_none() {
            return Err(Error::msg("Connection does not exist"));
        }
    }

    let meta = RemoteConnMeta {
        charger_id: uuid::Uuid::from_u128(response.charger_id),
        conn_no: response.connection_no,
    };

    {
        let mut map = state.undiscovered_clients.lock().unwrap();
        if let Some(r) = map.remove(&meta) {
            let mut map = state.web_client_map.lock().unwrap();
            map.insert(addr, r);
        }
    }

    let mut map = state.charger_remote_conn_map.lock().unwrap();
    map.insert(meta, addr);

    Ok(())
}
