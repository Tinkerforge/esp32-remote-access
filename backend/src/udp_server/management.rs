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

use crate::BridgeState;

#[derive(PartialEq, Hash, Eq, Debug)]
pub struct RemoteConnMeta {
    pub charger_id: i32,
    pub conn_no: i32,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub enum ManagementCommandId {
    Connect,
    Disconnect,
    Ack,
}

#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct ManagementCommand {
    pub command_id: ManagementCommandId,
    pub connection_no: i32,
    pub connection_uuid: u128,
}

#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct ManagementPacketHeader {
    pub magic: u16,
    pub length: u16,
    pub seq_number: u16,
    pub version: u8,
    /*
        0x00 - Management Command
    */
    pub p_type: u8,
}

#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct ManagementCommandPacket {
    pub header: ManagementPacketHeader,
    pub command: ManagementCommand,
}

#[derive(PartialEq, Eq, Hash, Debug, Clone, Copy)]
#[repr(C, packed)]
pub struct ManagementResponse {
    pub charger_id: i32,
    pub connection_no: i32,
    pub connection_uuid: u128,
}

pub fn try_port_discovery(state: &web::Data<BridgeState>, data: &[u8], addr: SocketAddr) -> bool {
    if data.len() != ::core::mem::size_of::<ManagementResponse>() {
        return false;
    }

    let response: ManagementResponse = unsafe {
        // using std::mem::transmute is more unsafe than std::ptr::read. https://users.rust-lang.org/t/isnt-a-pointer-cast-just-a-more-dangerous-transmute/47007
        std::ptr::read(data.as_ptr() as *const _)
    };

    {
        let mut set = state.port_discovery.lock().unwrap();
        if set.remove(&response).is_none() {
            return false;
        }
    }

    let meta = RemoteConnMeta {
        charger_id: response.charger_id,
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

    true
}
