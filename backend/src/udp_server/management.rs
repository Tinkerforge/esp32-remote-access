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
use serde::Serialize;

use crate::BridgeState;

use super::packet::ManagementResponse;

#[derive(PartialEq, Hash, Eq, Debug, Serialize, Clone)]
pub struct RemoteConnMeta {
    pub charger_id: i32,
    pub conn_no: i32,
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
