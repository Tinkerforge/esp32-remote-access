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
    rate_limit::ShrinkableRateLimiter, udp_server::multiplex::run_server,
    utils::update_charger_state_change, AppState, BridgeState, DiscoveryCharger,
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
    rate_limiters: Vec<Arc<dyn ShrinkableRateLimiter>>,
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
                device_map.remove(&addr);
            }
            device_map.shrink_to_fit();
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
            map.shrink_to_fit();
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
                    // Drop any buckets the cleared entries no longer need so
                    // the set's capacity tracks the live entries instead of
                    // the all-time high-water mark.
                    devices.shrink_to_fit();
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
            map.shrink_to_fit();
        }
        {
            let to_remove: Vec<uuid::Uuid> = {
                let mut device_map_id = device_map_id.lock().await;
                let mut to_remove = Vec::with_capacity(device_map_id.len());
                for (id, socket) in device_map_id.iter() {
                    let socket = socket.lock().await;
                    if socket.last_seen() > Duration::from_secs(30) {
                        to_remove.push(id.to_owned());
                    }
                }
                for id in to_remove.iter() {
                    device_map_id.remove(id);
                }
                to_remove
            };
            for id in to_remove.into_iter() {
                log::info!("Charger {id} has timeouted and will be removed.");
                update_charger_state_change(id, state.clone(), bridge_state.clone()).await;
            }
        }
        for limiter in rate_limiters.iter() {
            limiter.shrink();
        }
        // Periodically shrink the connection-tracking maps so their bucket
        // capacity tracks the live entries instead of the all-time
        // high-water mark. These maps are drained on every request by
        // multiplex/management/ws_udp_bridge but never compacted, so under
        // stress the buckets linger even after the entries are gone.
        {
            let mut map = bridge_state.web_client_map.lock().await;
            map.shrink_to_fit();
        }
        {
            let mut map = bridge_state.undiscovered_clients.lock().await;
            map.shrink_to_fit();
        }
        {
            let mut map = bridge_state.device_remote_conn_map.lock().await;
            map.shrink_to_fit();
        }
        {
            let mut map = bridge_state.lost_connections.lock().await;
            map.shrink_to_fit();
        }
        {
            let mut map = bridge_state.state_update_clients.lock().await;
            map.shrink_to_fit();
        }
        tokio::time::sleep(Duration::from_secs(10)).await;
    }
}

pub fn start_server(
    bridge_state: web::Data<BridgeState<'static>>,
    app_state: web::Data<AppState>,
    mut rate_limiters: Vec<Arc<dyn ShrinkableRateLimiter>>,
) {
    log::info!("Starting Wireguard server.");
    let global_search_rate_limiter = Arc::new(crate::rate_limit::GlobalSearchRateLimiter::new());
    rate_limiters.push(Arc::clone(&global_search_rate_limiter) as Arc<dyn ShrinkableRateLimiter>);
    actix::spawn(start_rate_limiters_reset_thread(
        bridge_state.device_management_map.clone(),
        bridge_state.device_management_map_with_id.clone(),
        bridge_state.port_discovery.clone(),
        bridge_state.undiscovered_devices.clone(),
        app_state.clone(),
        bridge_state.clone(),
        rate_limiters,
    ));

    actix::spawn(run_server(
        bridge_state,
        app_state,
        global_search_rate_limiter,
    ));
}
