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
    sync::{Arc, Mutex},
};

use actix_web::web::{self, Bytes};
use base64::prelude::*;
use boringtun::noise::{rate_limiter::RateLimiter, TunnResult};
use db_connector::models::chargers::Charger;
use diesel::prelude::*;
use ipnetwork::IpNetwork;
use rand::RngCore;
use rand_core::OsRng;
use threadpool::ThreadPool;

use crate::{
    udp_server::management::{ManagementCommandId, ManagementCommandPacket, ManagementPacketHeader}, utils::as_u8_slice, ws_udp_bridge::Message,
    BridgeState,
};

use super::{
    management::{try_port_discovery, ManagementCommand},
    socket::ManagementSocket,
    start_rate_limiters_reset_thread,
};

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

fn create_tunn(
    state: &web::Data<BridgeState>,
    addr: SocketAddr,
    data: &[u8],
) -> anyhow::Result<(i32, ManagementSocket)> {
    use db_connector::schema::chargers::dsl as chargers;

    let mut conn = state.pool.get()?;

    let ip = IpNetwork::new(addr.ip(), 32)?;
    let chargers: Vec<Charger> = chargers::chargers
        .filter(chargers::last_ip.eq(ip))
        .select(Charger::as_select())
        .load(&mut conn)?;

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

        let mut tunn = match boringtun::noise::Tunn::new(
            static_private,
            peer_static_public,
            Some(psk),
            Some(5),
            OsRng.next_u32(),
            Some(rate_limiter.clone()),
        ) {
            Ok(tunn) => tunn,
            Err(err) => return Err(anyhow::Error::msg(err)),
        };

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

        let udp_socket = state.socket.try_clone()?;
        let socket = ManagementSocket::new(
            self_ip,
            peer_ip,
            addr,
            tunn,
            rate_limiter,
            Arc::new(udp_socket),
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

pub fn run_server(state: web::Data<BridgeState>, thread_pool: ThreadPool) {
    start_rate_limiters_reset_thread(
        state.charger_management_map.clone(),
        state.charger_management_map_with_id.clone(),
        state.port_discovery.clone(),
    );

    let mut buf = vec![0u8; 65535];
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

            if try_port_discovery(&state, &buf[..s], addr) {
                continue;
            }

            let tunn_sock = {
                // Maybe we could release the lock when adding a new management connection and get it back later
                // in case it turns out that holding it causes major connection issues.
                let mut charger_map = state.charger_management_map.lock().unwrap();
                match charger_map.entry(addr) {
                    Entry::Occupied(tunn) => tunn.into_mut().clone(),
                    Entry::Vacant(v) => {
                        let (id, tunn_data) = match create_tunn(&state, addr, &buf[..s]) {
                            Ok(tunn) => tunn,
                            Err(_err) => {
                                continue;
                            }
                        };

                        let tunn_data = Arc::new(Mutex::new(tunn_data));
                        let mut map = state.charger_management_map_with_id.lock().unwrap();
                        map.insert(id, tunn_data.clone());
                        v.insert(tunn_data.clone());
                        let tunn = tunn_data.clone();
                        log::debug!("Adding management connection from {}", addr);
                        thread_pool.execute(move || {
                            let mut tunn = tunn.lock().unwrap();
                            for i in 0..5 {
                                let command = ManagementCommand {
                                    command_id: ManagementCommandId::Disconnect,
                                    connection_no: i,
                                    connection_uuid: uuid::Uuid::new_v4().as_u128(),
                                };
                                let header = ManagementPacketHeader {
                                    magic: 0x1234,
                                    length: std::mem::size_of::<ManagementCommand>() as u16,
                                    seq_number: 0,
                                    version: 1,
                                    p_type: 0x00
                                };

                                let packet = ManagementCommandPacket {
                                    header,
                                    command
                                };

                                tunn.encrypt_and_send_slice(as_u8_slice(&packet));
                            }
                        });
                        tunn_data
                    }
                }
            };

            let data = {
                let mut tun_sock = tunn_sock.lock().unwrap();
                match tun_sock.decrypt(&buf[..s]) {
                    Ok(data) => data,
                    Err(_) => {
                        continue;
                    }
                }
            };

            if data.len() == std::mem::size_of::<ManagementCommand>() {}
        } else {
        }
    }
}
