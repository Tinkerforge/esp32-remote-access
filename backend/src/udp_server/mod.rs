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

pub mod device;
pub mod management;
mod multiplex;
pub mod packet;
pub mod pcap_logger;
pub mod socket;

use futures_util::lock::Mutex;
use std::{
    collections::{HashMap, HashSet},
    net::SocketAddr,
    sync::Arc,
    time::{Duration, Instant},
};

use self::socket::ManagementSocket;
use crate::{
    udp_server::multiplex::run_server, utils::update_charger_state_change, AppState, BridgeState,
    DiscoveryCharger,
};
use actix_web::web;
use ipnetwork::IpNetwork;
use packet::ManagementResponseV2;

/// Since boringtun doesnt reset the internal ratelimiter for us we need to do it manually.
/// We can do this with a very low frequency since the management connection
/// is always one to one and the esps keepalive is two minutes.
async fn start_rate_limiters_reset_thread(
    device_map: Arc<Mutex<HashMap<SocketAddr, Arc<Mutex<ManagementSocket<'static>>>>>>,
    device_map_id: Arc<Mutex<HashMap<uuid::Uuid, Arc<Mutex<ManagementSocket<'static>>>>>>,
    discovery_map: Arc<Mutex<HashMap<ManagementResponseV2, Instant>>>,
    undiscovered_devices: Arc<Mutex<HashMap<IpNetwork, HashSet<DiscoveryCharger>>>>,
    state: web::Data<AppState>,
    bridge_state: web::Data<BridgeState<'static>>,
) {
    loop {
        {
            let mut device_map = device_map.lock().await;
            let mut to_remove = Vec::with_capacity(device_map.len());
            for (addr, socket) in device_map.iter() {
                let socket = socket.lock().await;
                if socket.last_seen() > Duration::from_secs(30) {
                    to_remove.push(addr.to_owned());
                    continue;
                }
                socket.reset_rate_limiter();
            }
            for addr in to_remove.into_iter() {
                let socket = device_map.remove(&addr).unwrap();
                let socket = socket.lock().await;
                let mut map = device_map_id.lock().await;
                let (remove, id) = if let Some(c) = map.get(&socket.id()) {
                    drop(socket);
                    let c = c.lock().await;
                    if c.last_seen() > Duration::from_secs(30) {
                        (true, c.id())
                    } else {
                        (false, uuid::Uuid::nil())
                    }
                } else {
                    (false, uuid::Uuid::nil())
                };
                if remove {
                    log::info!("Charger {id} has timeouted and will be removed.");
                    map.remove(&id);
                    drop(map);
                    update_charger_state_change(id, state.clone(), bridge_state.clone()).await;
                }
            }
        }
        {
            let mut map = discovery_map.lock().await;
            let mut to_remove = Vec::with_capacity(map.len());
            for (cmd, created) in map.iter() {
                if created.elapsed() > Duration::from_secs(30) {
                    to_remove.push(cmd.to_owned());
                }
            }
            for cmd in to_remove.iter() {
                map.remove(cmd);
            }
        }
        {
            let mut map = undiscovered_devices.lock().await;
            let to_remove: Vec<Option<IpNetwork>> = map
                .iter_mut()
                .map(|(ip, devices)| {
                    let to_remove: Vec<Option<DiscoveryCharger>> = devices
                        .iter()
                        .map(|device| {
                            if device.last_request.elapsed() > Duration::from_secs(60) {
                                Some(device.to_owned())
                            } else {
                                None
                            }
                        })
                        .collect();
                    for c in to_remove.iter().flatten() {
                        devices.remove(c);
                    }
                    if devices.is_empty() {
                        Some(ip.to_owned())
                    } else {
                        None
                    }
                })
                .collect();
            for ip in to_remove.into_iter().flatten() {
                map.remove(&ip);
            }
        }
        tokio::time::sleep(Duration::from_secs(10)).await;
    }
}

pub fn start_server(bridge_state: web::Data<BridgeState<'static>>, app_state: web::Data<AppState>) {
    log::info!("Starting Wireguard server.");
    actix::spawn(start_rate_limiters_reset_thread(
        bridge_state.device_management_map.clone(),
        bridge_state.device_management_map_with_id.clone(),
        bridge_state.port_discovery.clone(),
        bridge_state.undiscovered_devices.clone(),
        app_state.clone(),
        bridge_state.clone(),
    ));

    actix::spawn(run_server(bridge_state, app_state));
}
