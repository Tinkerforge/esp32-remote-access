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

use std::{
    collections::hash_map::Entry,
    net::{IpAddr, SocketAddr, UdpSocket},
    sync::Arc,
};

use actix::Arbiter;
use actix_web::web::{self, Bytes};
use base64::prelude::*;
use boringtun::noise::{rate_limiter::RateLimiter, TunnResult};
use db_connector::models::chargers::Charger;
use diesel::prelude::*;
use futures_util::lock::Mutex;
use ipnetwork::{IpNetwork, Ipv4Network};
use rand::TryRngCore;
use rand_core::OsRng;

use crate::{
    udp_server::{management::RemoteConnMeta, packet::ManagementCommand},
    utils::update_charger_state_change,
    ws_udp_bridge::open_connection,
    AppState, BridgeState,
};

use super::{management::try_port_discovery, socket::ManagementSocket};

#[derive(Debug)]
enum Error {
    UnknownPeer,
}

impl std::fmt::Display for Error {
    fn fmt(&self, _: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        Ok(())
    }
}

impl std::error::Error for Error {}

async fn create_tunn(
    state: &web::Data<BridgeState>,
    addr: SocketAddr,
    data: &[u8],
) -> anyhow::Result<(uuid::Uuid, ManagementSocket)> {
    use db_connector::schema::chargers::dsl as chargers;

    let mut conn = state.pool.get()?;

    let ip = IpNetwork::new(addr.ip(), 32)?;

    let chargers: Vec<Charger> = {
        let map = state.undiscovered_chargers.lock().await;
        if let Some(set) = map.get(&ip) {
            let charger_ids: Vec<uuid::Uuid> = set.iter().map(|c| c.id).collect();
            chargers::chargers
                .filter(chargers::id.eq_any(charger_ids))
                .select(Charger::as_select())
                .load(&mut conn)?
        } else {
            let IpNetwork::V4(ip) = ip else {
                return Err(anyhow::Error::msg(Error::UnknownPeer));
            };
            let subnet = Ipv4Network::new(ip.ip(), 24)?;
            let matching_entries = map
                .iter()
                .filter(|(network, _)| {
                    let ipv4_network = match *network {
                        IpNetwork::V4(ipv4) => ipv4,
                        _ => return false,
                    };
                    subnet.contains(ipv4_network.ip())
                })
                .collect::<Vec<_>>();
            if !matching_entries.is_empty() {
                let device_ids: Vec<uuid::Uuid> = matching_entries
                    .iter()
                    .flat_map(|(_, devices)| devices.iter().map(|d| d.id))
                    .collect();
                log::info!("Found possible matches for ip '{subnet}: {device_ids:?}'");
                chargers::chargers
                    .filter(chargers::id.eq_any(device_ids))
                    .select(Charger::as_select())
                    .load(&mut conn)?
            } else if Ipv4Network::new(std::env::var("FORWARD_HOST")?.parse()?, 32)? == ip {
                log::info!("Found forwarded management connection");
                let mut device_ids: Vec<uuid::Uuid> = Vec::new();
                for (_, devices) in map.iter() {
                    for device in devices.iter() {
                        device_ids.push(device.id);
                    }
                }
                chargers::chargers
                    .filter(chargers::id.eq_any(device_ids))
                    .select(Charger::as_select())
                    .load(&mut conn)?
            } else {
                log::info!("Could not find charger for ip '{subnet}'");
                return Err(anyhow::Error::msg(Error::UnknownPeer));
            }
        }
    };

    let mut dst = vec![0u8; data.len()];
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
        let peer_static_public: [u8; 32] =
            match BASE64_STANDARD.decode(charger.charger_pub)?.try_into() {
                Ok(v) => v,
                Err(_) => {
                    return Err(anyhow::Error::msg(
                        "Somehow we got an invalid charger public key in the database.",
                    ))
                }
            };

        let static_private = boringtun::x25519::StaticSecret::from(static_private);
        let peer_static_public = boringtun::x25519::PublicKey::from(peer_static_public);

        let rate_limiter = Arc::new(RateLimiter::new(
            &boringtun::x25519::PublicKey::from(&static_private),
            10,
        ));

        let psk = BASE64_STANDARD.decode(charger.psk)?;
        let psk = match psk.try_into() {
            Ok(psk) => psk,
            Err(_err) => return Err(anyhow::Error::msg("Database is corrupted")),
        };

        let mut tunn = boringtun::noise::Tunn::new(
            static_private,
            peer_static_public,
            Some(psk),
            Some(5),
            OsRng.try_next_u32()?,
            Some(rate_limiter.clone()),
        );

        match tunn.decapsulate(None, data, &mut dst) {
            TunnResult::WriteToNetwork(data) => {
                send_data(&state.socket, addr, data);
            }
            _ => continue,
        }

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

        let udp_socket = Arc::clone(&state.socket);
        let socket = ManagementSocket::new(
            self_ip,
            peer_ip,
            addr,
            tunn,
            rate_limiter,
            udp_socket,
            charger.id,
        );
        return Ok((charger.id, socket));
    }

    Err(anyhow::Error::new(Error::UnknownPeer))
}

pub fn send_data(socket: &UdpSocket, addr: SocketAddr, data: &[u8]) {
    match socket.send_to(data, addr) {
        Ok(s) => {
            if s < data.len() {
                log::error!("Sent incomplete datagram to charger with ip '{addr}'");
            }
        }
        Err(err) => {
            log::error!("Failed to send datagram to charger with ip '{addr}': {err}");
        }
    }
}

pub async fn run_server(
    bridge_state: web::Data<BridgeState>,
    app_state: web::Data<AppState>,
    arbiter: Arc<Arbiter>,
) {
    let mut buf = vec![0u8; 65535];
    loop {
        if let Ok((s, addr)) = bridge_state.socket.recv_from(&mut buf) {
            let bridge_state = bridge_state.clone();
            let buf = buf.clone();

            // Check if the packet is for port discovery
            if try_port_discovery(&bridge_state, &buf[..s], addr)
                .await
                .is_ok()
            {
                continue;
            }

            // Check if we need to relay the packet
            {
                let mut client_map = bridge_state.web_client_map.lock().await;
                if let Some(client) = client_map.get_mut(&addr) {
                    let payload = Bytes::copy_from_slice(&buf[0..s]);
                    client.binary(payload).await.ok();
                    continue;
                }
            }

            // Get the management socket or create a new one when it does not exist
            let tunn_sock = {
                // Maybe we could release the lock when adding a new management connection and get it back later
                // in case it turns out that holding it causes major connection issues.
                let mut charger_map = bridge_state.charger_management_map.lock().await;
                match charger_map.entry(addr) {
                    Entry::Occupied(tunn) => tunn.into_mut().clone(),
                    Entry::Vacant(v) => {
                        let (id, tunn_data) =
                            match create_tunn(&bridge_state, addr, &buf[..s]).await {
                                Ok(tunn) => tunn,
                                Err(_err) => {
                                    continue;
                                }
                            };

                        arbiter.spawn(update_charger_state_change(
                            id,
                            app_state.clone(),
                            bridge_state.clone(),
                        ));

                        let tunn_data = Arc::new(Mutex::new(tunn_data));
                        let mut map = bridge_state.charger_management_map_with_id.lock().await;
                        map.insert(id, tunn_data.clone());
                        v.insert(tunn_data.clone());
                        let tunn = tunn_data.clone();
                        let mut lost_map = bridge_state.lost_connections.lock().await;
                        let mut undiscovered_clients =
                            bridge_state.undiscovered_clients.lock().await;
                        if let Some(conns) = lost_map.remove(&id) {
                            for (conn_no, recipient) in conns.into_iter() {
                                let meta = RemoteConnMeta {
                                    charger_id: id,
                                    conn_no,
                                };
                                undiscovered_clients.insert(meta, recipient);

                                open_connection(
                                    conn_no,
                                    id,
                                    tunn.clone(),
                                    bridge_state.port_discovery.clone(),
                                )
                                .await
                                .ok();
                            }
                        }
                        log::info!("Adding management connection from {addr}");
                        tunn_data.clone()
                    }
                }
            };

            let data = {
                let mut tun_sock = tunn_sock.lock().await;
                match tun_sock.decrypt(&buf[..s]) {
                    Ok(data) => data,
                    Err(_) => {
                        continue;
                    }
                }
            };

            if data.len() == std::mem::size_of::<ManagementCommand>() {}
        }
    }
}
