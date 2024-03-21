mod multiplex;
pub mod device;
pub mod management;
pub mod socket;

use std::{
    collections::HashMap, net::SocketAddr, sync::{Arc, Mutex}, time::Duration
};

use actix_web::web;
use threadpool::ThreadPool;
use crate::{udp_server::multiplex::run_server, BridgeState};

use self::socket::ManagementSocket;


/// Since boringtun doesnt reset the internal ratelimiter for us we need to do it manually.
/// We can do this with a very low frequency since the management connection
/// is always one to one and the esps keepalive is two minutes.
fn start_rate_limiters_reset_thread(charger_map: Arc<Mutex<HashMap<SocketAddr, Arc<Mutex<ManagementSocket>>>>>) {
    std::thread::spawn(move || {
        loop {
            {
                let charger_map = charger_map.lock().unwrap();
                for (_, charger) in charger_map.iter() {
                    let charger = charger.lock().unwrap();
                    charger.reset_rate_limiter();
                }
            }
            std::thread::sleep(Duration::from_secs(60));
        }
    });
}

pub fn start_server(state: web::Data<BridgeState>) -> std::io::Result<()> {
    log::info!("Starting Wireguard server.");
    let cpu_count = num_cpus::get();
    let thread_pool = ThreadPool::new(cpu_count / 2);
    std::thread::spawn(move || run_server(state, thread_pool));

    Ok(())
}
