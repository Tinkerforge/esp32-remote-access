pub mod device;
pub mod management;
mod multiplex;
pub mod socket;

use std::{
    collections::HashMap,
    net::SocketAddr,
    sync::{Arc, Mutex},
    time::Duration,
};

use crate::{udp_server::multiplex::run_server, BridgeState};
use actix_web::web;
use threadpool::ThreadPool;

use self::socket::ManagementSocket;

/// Since boringtun doesnt reset the internal ratelimiter for us we need to do it manually.
/// We can do this with a very low frequency since the management connection
/// is always one to one and the esps keepalive is two minutes.
fn start_rate_limiters_reset_thread(
    charger_map: Arc<Mutex<HashMap<SocketAddr, Arc<Mutex<ManagementSocket>>>>>,
    charger_map_id: Arc<Mutex<HashMap<i32, Arc<Mutex<ManagementSocket>>>>>,
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
                let charger = charger_map.remove(&addr).unwrap();
                let charger = charger.lock().unwrap();
                let mut map = charger_map_id.lock().unwrap();
                map.remove(&charger.id());
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
