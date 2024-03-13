use std::{
    collections::{hash_map::Entry, HashMap},
    net::{IpAddr, Ipv4Addr, SocketAddr, UdpSocket},
    time::Instant,
};

use actix_web::web::{self, Bytes};
use base64::prelude::*;
use boringtun::noise::{errors::WireGuardError, Tunn, TunnResult};
use db_connector::models::chargers::Charger;
use db_connector::schema::chargers::dsl as chargers;
use diesel::prelude::*;
use ipnetwork::IpNetwork;
use rand::RngCore;
use rand_core::OsRng;

use crate::{ws_udp_bridge::Message, BridgeState};

pub struct TunnData {
    tunn: Tunn,
    self_ip: Ipv4Addr,
    peer_ip: Ipv4Addr,
    last_seen: Instant,
}

enum Error {
    UnknownPeer,
    WireGuardError(WireGuardError),
}

fn create_tunn(state: &web::Data<BridgeState>, addr: SocketAddr) -> anyhow::Result<Vec<TunnData>> {
    let mut conn = state.pool.get()?;

    let ip = IpNetwork::new(addr.ip(), 32)?;
    let chargers: Vec<Charger> = chargers::chargers
        .filter(chargers::last_ip.eq(ip))
        .select(Charger::as_select())
        .load(&mut conn)?;

    let mut tunn_data = Vec::with_capacity(chargers.len());
    for charger in chargers.into_iter() {
        let static_private: [u8; 32] = match BASE64_STANDARD
            .decode(charger.management_private)?
            .try_into()
        {
            Ok(v) => v,
            Err(_) => {
                return Err(anyhow::Error::msg(
                    "Somehow we got an invalid server private key in the database.",
                ))
            }
        };
        let peer_static_public: [u8; 32] = match BASE64_STANDARD.decode(charger.charger_pub)?.try_into()
        {
            Ok(v) => v,
            Err(_) => {
                return Err(anyhow::Error::msg(
                    "Somehow we got an invalid charger public key in the database.",
                ))
            }
        };

        let static_private = boringtun::x25519::StaticSecret::from(static_private);
        let peer_static_public = boringtun::x25519::PublicKey::from(peer_static_public);

        // FIXME: we should add a ratelimiter here
        let tunn = match boringtun::noise::Tunn::new(
            static_private,
            peer_static_public,
            None,
            None,
            OsRng.next_u32(),
            None,
        ) {
            Ok(tunn) => tunn,
            Err(err) => return Err(anyhow::Error::msg(err)),
        };

        let self_ip = if let IpAddr::V4(ip) = charger.wg_server_ip.ip() {
            ip
        } else {
            return Err(anyhow::Error::msg(
                "Somehow a IPv6 address got into the database",
            ));
        };

        let peer_ip = if let IpAddr::V4(ip) = charger.wg_charger_ip.ip() {
            ip
        } else {
            return Err(anyhow::Error::msg(
                "Somehow a IPv6 address got into the database",
            ));
        };

        tunn_data.push(TunnData {
            tunn,
            last_seen: Instant::now(),
            self_ip,
            peer_ip,
        });
    }

    Ok(tunn_data)
}

fn send_data(socket: &UdpSocket, addr: SocketAddr, data: &[u8]) {
    match socket.send_to(data, addr) {
        Ok(s) => {
            if s < data.len() {
                log::error!(
                    "Sent incomplete datagram to charger with ip '{}'",
                    addr.to_string()
                );
            }
        }
        Err(err) => {
            log::error!(
                "Failed to send datagram to charger with ip '{}': {}",
                addr.to_string(),
                err
            );
        }
    }
}

fn decrypt_packet(tunn_data: &mut TunnData, data: &[u8], socket: &UdpSocket, addr: SocketAddr) -> Result<Vec<u8>, Error> {
    let mut dst = vec![0u8; data.len() + 32];
    match tunn_data.tunn.decapsulate(None, data, &mut dst) {
        TunnResult::WriteToNetwork(data) => {
            send_data(socket, addr, data);
            let tmp = [0u8; 0];

            while let TunnResult::WriteToNetwork(data) =
                tunn_data.tunn.decapsulate(None, &tmp, &mut dst)
            {
                send_data(socket, addr, data);
            }
            Ok(Vec::new())
        }
        TunnResult::WriteToTunnelV4(data, dest) => {
            if dest == tunn_data.self_ip {
                tunn_data.last_seen = Instant::now();
            }

            Ok(data.to_vec())
        }
        // There is currently no need for IPv6 inside the Wireguard network.
        TunnResult::WriteToTunnelV6(_, _) => {
            log::warn!("Someone with a valid key tried to connect to an IPv6 address");
            Err(Error::UnknownPeer)
        }
        TunnResult::Done => Ok(Vec::new()),
        TunnResult::Err(err) => {
            if let WireGuardError::InvalidMac = err {
                Err(Error::UnknownPeer)
            } else {
                Err(Error::WireGuardError(err))
            }
        },
    }
}

fn run_server(state: web::Data<BridgeState>) {
    let mut charger_map: HashMap<SocketAddr, TunnData> = HashMap::new();
    let charger_map = &mut charger_map;
    let mut buf = [0u8; 100000];
    loop {
        if let Ok((s, addr)) = state.socket.recv_from(&mut buf) {
            {
                let client_map = state.web_client_map.lock().unwrap();
                if let Some(client) = client_map.get(&addr) {
                    let payload = Bytes::copy_from_slice(&buf[0..s]);
                    let msg = Message(payload);
                    client.do_send(msg);
                    continue;
                }
            }

            let tunn_data = match charger_map.entry(addr) {
                Entry::Occupied(tunn) => tunn.into_mut(),
                Entry::Vacant(v) => {
                    let tunn_data = match create_tunn(&state, addr) {
                        Ok(tunn) => tunn,
                        Err(_err) => {
                            continue;
                        }
                    };

                    let mut ret = None;
                    for mut tunn_data in tunn_data.into_iter() {
                        if decrypt_packet(&mut tunn_data, &buf[0..s], &state.socket, addr).is_err() {
                            continue;
                        }
                        ret = Some(v.insert(tunn_data));
                        break;
                    }

                    if let Some(tunn_data) = ret {
                        tunn_data
                    } else {
                        continue;
                    }
                }
            };

            let res = decrypt_packet(tunn_data, &buf[0..s], &state.socket, addr);
        } else {
        }
    }
}

pub fn start_server(state: web::Data<BridgeState>) -> std::io::Result<()> {
    log::info!("Starting Wireguard server.");
    std::thread::spawn(move || run_server(state));

    Ok(())
}
