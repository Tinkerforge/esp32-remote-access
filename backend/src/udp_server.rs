use std::{collections::{hash_map::Entry, HashMap}, net::{IpAddr, Ipv4Addr, SocketAddr, UdpSocket}, time::Instant};

use actix_web::web;
use boringtun::noise::{Tunn, TunnResult};
use db_connector::models::chargers::Charger;
use db_connector::schema::chargers::dsl as chargers;
use diesel::prelude::*;
use ipnetwork::IpNetwork;
use rand::RngCore;
use rand_core::OsRng;
use base64::prelude::*;

use crate::BridgeState;

pub struct TunnData {
    tunn: Tunn,
    self_ip: Ipv4Addr,
    peer_ip: Ipv4Addr,
    last_seen: Instant,
}

fn create_tunn(state: &web::Data<BridgeState>, addr: SocketAddr) -> anyhow::Result<TunnData> {
    let mut conn = state.pool.get()?;

    let ip = IpNetwork::new(addr.ip(), addr.port().try_into()?)?;
    let charger: Charger = chargers::chargers.filter(chargers::last_ip.eq(ip)).select(Charger::as_select())
        .get_result(&mut conn)?;

    let static_private: [u8; 32] = match BASE64_STANDARD.decode(charger.management_private)?.try_into() {
        Ok(v) => v,
        Err(_) => {
            return Err(anyhow::Error::msg("Somehow we got an invalid server private key in the database."))
        }
    };
    let peer_static_public: [u8; 32] = match BASE64_STANDARD.decode(charger.charger_pub)?.try_into() {
        Ok(v) => v,
        Err(_) => {
            return Err(anyhow::Error::msg("Somehow we got an invalid charger public key in the database."))
        }
    };

    let static_private = boringtun::x25519::StaticSecret::from(static_private);
    let peer_static_public = boringtun::x25519::PublicKey::from(peer_static_public);

    // FIXME: we should add a ratelimiter here
    let tunn = match boringtun::noise::Tunn::new(static_private, peer_static_public, None, None, OsRng.next_u32(), None) {
        Ok(tunn) => tunn,
        Err(err) => {
            return Err(anyhow::Error::msg(err))
        }
    };

    let self_ip = if let IpAddr::V4(ip) = charger.wg_server_ip.ip() {
        ip
    } else {
        return Err(anyhow::Error::msg("Somehow a IPv6 address got into the database"))
    };

    let peer_ip = if let IpAddr::V4(ip) = charger.wg_charger_ip.ip() {
        ip
    } else {
        return  Err(anyhow::Error::msg("Somehow a IPv6 address got into the database"))
    };

    Ok(TunnData {
        tunn,
        last_seen: Instant::now(),
        self_ip,
        peer_ip,
    })
}

fn run_server(state: web::Data<BridgeState>, sock: UdpSocket) {
    let mut charger_map: HashMap<SocketAddr, TunnData> = HashMap::new();
    let charger_map = &mut charger_map;
    let mut buf = [0u8; 100000];
    loop {
        if let Ok((s, addr)) = sock.recv_from(&mut buf) {
            let tunn_data = match charger_map.entry(addr) {
                Entry::Occupied(tunn) => tunn.into_mut(),
                Entry::Vacant(v) => {
                    let tunn_data = match create_tunn(&state, addr) {
                        Ok(tunn) => tunn,
                        Err(err) => {
                            log::error!("Failed to create new Management connection: {}", err);
                            continue;
                        }
                    };
                    v.insert(tunn_data)
                }
            };

            let mut buf2 = vec![0u8; s + 32];
            match tunn_data.tunn.decapsulate(None, &buf[0..s], &mut buf2) {
                TunnResult::WriteToNetwork(data) => {
                    match sock.send_to(&data, addr) {
                        Ok(s) => {
                            if s < buf2.len() {
                                log::error!("Sent incomplete datagram to charger with ip '{}'", addr.to_string());
                            }
                        },
                        Err(err) => {
                            log::error!("Failed to send datagram to charger with ip '{}': {}", addr.to_string(), err);
                        }
                    }
                },
                TunnResult::WriteToTunnelV4(data, dest) => {
                    if dest == tunn_data.self_ip {
                        tunn_data.last_seen = Instant::now();
                        log::info!("Got data: {:?}", data);
                    }
                },
                // There is currently no need for IPv6 inside the Wireguard network.
                TunnResult::WriteToTunnelV6(_, _) => {
                    log::warn!("Someone with a valid key tried to connect to an IPv6 address");
                },
                TunnResult::Done => (),
                TunnResult::Err(err) => {
                    log::error!("Error while decrypting packet: {:?}", err);
                }
            }
        } else {
        }
    }
}

pub fn start_server(state: web::Data<BridgeState>) -> std::io::Result<()> {
    log::info!("Starting Wireguard server.");
    let sock = UdpSocket::bind("0.0.0.0:51820")?;
    std::thread::spawn(move || {
        run_server(state, sock)
    });

    Ok(())
}
