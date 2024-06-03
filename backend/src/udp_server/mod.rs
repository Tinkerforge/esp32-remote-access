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
pub mod socket;
pub mod packet;

use std::{
    collections::HashMap,
    net::SocketAddr,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use crate::{udp_server::multiplex::run_server, BridgeState};
use actix_web::web;
use packet::ManagementResponse;
use threadpool::ThreadPool;

use self::socket::ManagementSocket;

/// Since boringtun doesnt reset the internal ratelimiter for us we need to do it manually.
/// We can do this with a very low frequency since the management connection
/// is always one to one and the esps keepalive is two minutes.
fn start_rate_limiters_reset_thread(
    charger_map: Arc<Mutex<HashMap<SocketAddr, Arc<Mutex<ManagementSocket>>>>>,
    charger_map_id: Arc<Mutex<HashMap<i32, Arc<Mutex<ManagementSocket>>>>>,
    discovery_map: Arc<Mutex<HashMap<ManagementResponse, Instant>>>,
) {
    std::thread::spawn(move || loop {
        {
            let mut charger_map = charger_map.lock().unwrap();
            let mut to_remove = Vec::with_capacity(charger_map.len());
            for (addr, charger) in charger_map.iter() {
                let charger = charger.lock().unwrap();
                if charger.last_seen() > Duration::from_secs(30) {
                    to_remove.push(addr.to_owned());
                    continue;
                }
                charger.reset_rate_limiter();
            }
            for addr in to_remove.into_iter() {
                log::debug!("Charger {} has timeouted and will be removed.", addr);
                let charger = charger_map.remove(&addr).unwrap();
                let charger = charger.lock().unwrap();
                let mut map = charger_map_id.lock().unwrap();
                map.remove(&charger.id());
            }
        }
        {
            let mut map = discovery_map.lock().unwrap();
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
        std::thread::sleep(Duration::from_secs(10));
    });
}

pub fn start_server(state: web::Data<BridgeState>) -> std::io::Result<()> {
    log::info!("Starting Wireguard server.");
    let cpu_count = num_cpus::get();
    let thread_pool = ThreadPool::new(cpu_count / 2);
    std::thread::spawn(move || run_server(state, thread_pool));

    Ok(())
}
