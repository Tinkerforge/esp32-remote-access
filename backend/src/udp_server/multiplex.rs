/* esp32-remote-access
 * Copyright (C) 2026 Frederic Henrichs <frederic@tinkerforge.com>
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
    io::{BufWriter, Write},
    net::{IpAddr, SocketAddr},
    sync::{atomic::AtomicUsize, Arc},
    time::Duration,
};
use tokio::{net::UdpSocket, sync::Semaphore};

use actix_web::web::{self, Bytes};
use base64::prelude::*;
use boringtun::noise::{rate_limiter::RateLimiter, TunnResult};
use db_connector::models::chargers::Charger;
use diesel::{ExpressionMethods, QueryDsl, SelectableHelper};
use diesel_async::RunQueryDsl as _;
use futures_util::lock::Mutex;
use ipnetwork::{IpNetwork, Ipv4Network};
use rand_core::{OsRng, TryRngCore};

use crate::{
    rate_limit::GlobalSearchRateLimiter,
    routes::{charger::user_is_allowed, send_chargelog_to_user::send_charge_log_to_user},
    udp_server::{
        management::RemoteConnMeta,
        packet::{
            extract_management_packet_header, AckPacket, ChargeLogSendMetadata,
            ChargeLogSendMetadataPacket, ManagementPacket, NackPacket, NackReason, PacketType,
            RequestChargeLogSendPacket,
        },
    },
    utils::{
        get_last_charge_log_upload_hash, set_last_charge_log_upload_hash,
        update_charger_state_change,
    },
    ws_udp_bridge::open_connection,
    AppState, BridgeState,
};

use super::{
    admission::{DiscoveryAdmission, DiscoveryPermit},
    management::try_port_discovery,
    socket::{ManagementSocket, ManagementSocketTCPReceiver, TCPRecvResult},
};

static CURRENT_CHARGE_LOG_SENDS: AtomicUsize = AtomicUsize::new(0);

struct CurrentChargeLogSendsRAII;

impl CurrentChargeLogSendsRAII {
    fn new() -> anyhow::Result<Self> {
        match CURRENT_CHARGE_LOG_SENDS.fetch_update(
            std::sync::atomic::Ordering::SeqCst,
            std::sync::atomic::Ordering::SeqCst,
            |x| {
                if x < 250 {
                    Some(x + 1)
                } else {
                    None
                }
            },
        ) {
            Ok(_) => Ok(Self),
            Err(_) => Err(anyhow::Error::msg("Too many concurrent charge log sends")),
        }
    }
}

impl Drop for CurrentChargeLogSendsRAII {
    fn drop(&mut self) {
        let _ = CURRENT_CHARGE_LOG_SENDS.fetch_update(
            std::sync::atomic::Ordering::SeqCst,
            std::sync::atomic::Ordering::SeqCst,
            |x| {
                if x > 0 {
                    Some(x - 1)
                } else {
                    None
                }
            },
        );
    }
}

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
    state: &web::Data<BridgeState<'_>>,
    addr: SocketAddr,
    data: &[u8],
    rate_limiter: Arc<GlobalSearchRateLimiter>,
    permit: Arc<DiscoveryPermit>,
) -> anyhow::Result<(uuid::Uuid, ManagementSocket<'static>, Vec<u8>)> {
    use db_connector::schema::chargers::dsl as chargers;

    if !is_handshake_initiation(data) {
        return Err(anyhow::Error::new(Error::UnknownPeer));
    }
    let ip = IpNetwork::new(addr.ip(), 32)?;

    // Decide which chargers to load based on the in-memory
    // `undiscovered_devices` map. We collect the IDs (or note that we
    // want *all* chargers for an unknown peer) while holding the map's
    // lock, then release the lock before touching the DB pool so we
    // don't hold the mutex across `.await`.
    enum ChargerSelector {
        Ids(Vec<uuid::Uuid>),
        All,
    }

    let selector: ChargerSelector = {
        let map = state.undiscovered_devices.lock().await;
        if let Some(set) = map.get(&ip) {
            ChargerSelector::Ids(set.iter().map(|c| c.id).collect())
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
                ChargerSelector::Ids(device_ids)
            } else if Ipv4Network::new(std::env::var("FORWARD_HOST")?.parse()?, 32)? == ip {
                log::info!("Found forwarded management connection");
                let mut device_ids: Vec<uuid::Uuid> = Vec::new();
                for devices in map.iter() {
                    for device in devices.1.iter() {
                        device_ids.push(device.id);
                    }
                }
                ChargerSelector::Ids(device_ids)
            } else {
                if rate_limiter.check(addr).is_err() {
                    return Err(anyhow::Error::msg(Error::UnknownPeer));
                }
                ChargerSelector::All
            }
        }
    };

    // Load the candidate chargers and immediately release the pool slot.
    // Everything below this point is CPU-heavy noise key work that has
    // nothing to do with the database.
    let devices: Vec<Charger> = tokio::time::timeout(Duration::from_secs(10), async {
        let mut conn = state.pool.get().await?;
        let devices = match &selector {
            ChargerSelector::Ids(ids) => {
                chargers::chargers
                    .filter(chargers::id.eq_any(ids))
                    .select(Charger::as_select())
                    .load(&mut conn)
                    .await?
            }
            ChargerSelector::All => {
                chargers::chargers
                    .select(Charger::as_select())
                    .load(&mut conn)
                    .await?
            }
        };
        Ok::<Vec<Charger>, anyhow::Error>(devices)
    })
    .await??;

    let data = data.to_vec();
    let udp_socket = state.socket.clone();
    tokio::task::spawn_blocking(move || {
        // Keep admission reserved even if the async caller is cancelled.
        let _permit = permit;
        match_tunnel(devices, addr, &data, udp_socket)
    })
    .await?
}

fn is_handshake_initiation(data: &[u8]) -> bool {
    matches!(
        boringtun::noise::Tunn::parse_incoming_packet(data),
        Ok(boringtun::noise::Packet::HandshakeInit(_))
    )
}

fn match_tunnel(
    devices: Vec<Charger>,
    addr: SocketAddr,
    data: &[u8],
    udp_socket: Arc<UdpSocket>,
) -> anyhow::Result<(uuid::Uuid, ManagementSocket<'static>, Vec<u8>)> {
    let mut dst = vec![0u8; data.len()];
    for device in devices.into_iter() {
        let static_private: [u8; 32] = match BASE64_STANDARD
            .decode(device.management_private)?
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
            match BASE64_STANDARD.decode(device.charger_pub)?.try_into() {
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

        let psk = BASE64_STANDARD.decode(device.psk)?;
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

        let response = match tunn.decapsulate(None, data, &mut dst) {
            TunnResult::WriteToNetwork(data) => data.to_vec(),
            _ => continue,
        };

        let self_ip = if let IpAddr::V4(ip) = device.wg_server_ip.ip() {
            ip
        } else {
            return Err(anyhow::Error::msg(
                "Somehow a IPv6 address got into the database",
            ));
        };

        let peer_ip = if let IpAddr::V4(ip) = device.wg_charger_ip.ip() {
            ip
        } else {
            return Err(anyhow::Error::msg(
                "Somehow a IPv6 address got into the database",
            ));
        };

        let socket = ManagementSocket::new(
            self_ip,
            peer_ip,
            addr,
            tunn,
            rate_limiter,
            udp_socket,
            device.id,
        );

        #[cfg(feature = "pcap-logging")]
        {
            let pcap_path = std::path::PathBuf::from(format!("pcap/device_{}.pcapng", device.id));
            if let Err(e) = socket.enable_pcap_logging(pcap_path.clone()) {
                log::error!(
                    "Failed to enable pcap logging for device {}: {}",
                    device.id,
                    e
                );
            } else {
                log::error!(
                    "Enabled pcap logging for device {} at {:?}",
                    device.id,
                    pcap_path
                );
            }
        }

        return Ok((device.id, socket, response));
    }

    Err(anyhow::Error::new(Error::UnknownPeer))
}

pub fn send_data(socket: &UdpSocket, addr: SocketAddr, data: &[u8]) {
    match socket.try_send_to(data, addr) {
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

async fn handle_charge_log<'a>(
    meta_data: ChargeLogSendMetadata,
    tunn_sock: Arc<Mutex<ManagementSocket<'a>>>,
    app_state: web::Data<AppState>,
) -> anyhow::Result<()> {
    let tcp_receiver = ManagementSocketTCPReceiver::new(tunn_sock.clone()).await;
    let ack_packet = ManagementPacket::AckPacket(AckPacket::new());
    {
        let mut tun_sock = tunn_sock.lock().await;
        tun_sock.send_packet(ack_packet);
    }

    let mut buf = BufWriter::new(Vec::with_capacity(10 * 1024 * 1024));
    loop {
        let handle_tcp_fut = tcp_receiver.handle_tcp_recv();
        tokio::select! {
            res = handle_tcp_fut => {
                match res {
                    TCPRecvResult::Ok(data) => {
                        buf.write_all(&data)
                            .map_err(|e| anyhow::Error::msg(format!("Error writing to charge log buffer: {}", e)))?;
                    },
                    TCPRecvResult::Finished => {
                        buf.flush()
                            .map_err(|e| anyhow::Error::msg(format!("Error flushing charge log buffer: {}", e)))?;
                        break;
                    }
                    TCPRecvResult::Err(e) => {
                        return Err(anyhow::Error::msg(format!("Error receiving from TCP socket: {}", e)));
                    }
                }
            },
            _ = tokio::time::sleep(std::time::Duration::from_secs(60)) => {
                let mut tun_sock = tunn_sock.lock().await;
                tun_sock.remove_tcp_socket();
                return Err(anyhow::Error::msg("Timeout while waiting for charge log data"));
            }
        }
    }

    // Send the charge log to the user
    let device_uuid = {
        let tunn_sock_lock = tunn_sock.lock().await;
        tunn_sock_lock.id()
    };
    send_charge_log_to_user(
        device_uuid,
        &meta_data,
        buf.into_inner().unwrap(),
        &app_state,
    )
    .await?;

    Ok(())
}

pub async fn run_server(
    bridge_state: web::Data<BridgeState<'static>>,
    app_state: web::Data<AppState>,
    rate_limiter: Arc<GlobalSearchRateLimiter>,
) {
    // Acquire before spawning: waiting tasks must not become an unbounded queue.
    let packet_slots = Arc::new(Semaphore::new(1024));
    let discovery = Arc::new(DiscoveryAdmission::new(
        (num_cpus::get_physical() / 2).clamp(1, 4),
    ));
    let mut buf = vec![0u8; 65535];
    loop {
        let rate_limiter = Arc::clone(&rate_limiter);
        if let Ok((s, addr)) = bridge_state.socket.recv_from(&mut buf).await {
            let bridge_state = bridge_state.clone();
            let app_state = app_state.clone();
            let Ok(packet_slot) = packet_slots.clone().try_acquire_owned() else {
                continue;
            };
            let discovery = discovery.clone();
            let buf = buf[..s].to_vec();

            tokio::spawn(async move {
                let _packet_slot = packet_slot;
                // Check if the packet is for port discovery
                if try_port_discovery(&bridge_state, &buf[..s], addr)
                    .await
                    .is_ok()
                {
                    return;
                }

                // A slow browser must not hold the global map or retain a
                // packet task indefinitely.
                let client = bridge_state.web_client_map.lock().await.get(&addr).cloned();
                if let Some(mut client) = client {
                    let _ = tokio::time::timeout(
                        Duration::from_secs(2),
                        client.binary(Bytes::from(buf)),
                    )
                    .await;
                    return;
                }

                let existing = bridge_state
                    .device_management_map
                    .lock()
                    .await
                    .get(&addr)
                    .cloned();
                let tunn_sock = if let Some(socket) = existing {
                    socket
                } else {
                    if !is_handshake_initiation(&buf) {
                        return;
                    }
                    let Some(permit) = discovery.try_acquire(addr) else {
                        return;
                    };
                    let (id, socket, response) =
                        match create_tunn(&bridge_state, addr, &buf, rate_limiter, permit.clone())
                            .await
                        {
                            Ok(tunnel) => tunnel,
                            Err(err) => {
                                log::debug!("Tunnel discovery failed for {addr}: {err}");
                                return;
                            }
                        };
                    let Some(socket) = super::registry::publish(
                        &bridge_state.device_management_map,
                        &bridge_state.device_management_map_with_id,
                        id,
                        addr,
                        socket,
                        permit.generation,
                    )
                    .await
                    else {
                        // A newer handshake won, or the current tunnel is busy.
                        // Do not send a response for an unpublished tunnel.
                        return;
                    };
                    send_data(&bridge_state.socket, addr, &response);
                    // Keep discovery admission through reconnection bookkeeping
                    // so slow notifications cannot accumulate packet tasks.
                    let lost = bridge_state.lost_connections.lock().await.remove(&id);
                    if let Some(conns) = lost {
                        for (conn_no, recipient) in conns {
                            let meta = RemoteConnMeta {
                                charger_id: id,
                                conn_no,
                            };
                            bridge_state
                                .undiscovered_clients
                                .lock()
                                .await
                                .insert(meta, recipient);
                            open_connection(
                                conn_no,
                                id,
                                socket.clone(),
                                bridge_state.port_discovery.clone(),
                            )
                            .await
                            .ok();
                        }
                    }
                    let _ = tokio::time::timeout(
                        Duration::from_secs(5),
                        update_charger_state_change(id, app_state.clone(), bridge_state.clone()),
                    )
                    .await;
                    // create_tunn has already consumed the initiation packet.
                    return;
                };

                let (data, id) = {
                    let mut tun_sock = tunn_sock.lock().await;

                    // Decrypts wireguard packet, returning udp payload directly and pushing tcp payloads
                    // into the tcp socket inside the tunn object.
                    match tun_sock.decrypt(&buf[..s]) {
                        Ok(data) => (data, tun_sock.id()),
                        Err(_) => {
                            return;
                        }
                    }
                };

                let Ok(header) = extract_management_packet_header(&data, id) else {
                    return;
                };

                match header.p_type {
                    // Charge log send metadata packet
                    PacketType::MetadataForChargeLog => {
                        if let Ok(meta_data) =
                            ChargeLogSendMetadataPacket::try_from(data.as_slice())
                        {
                            let user_uuid = uuid::Uuid::from_u128(meta_data.data.user_uuid);
                            let sender = {
                                let mut tun_sock = tunn_sock.lock().await;
                                tun_sock.take_sender()
                            };

                            // Check if the user is allowed to access this charger
                            if let Err(e) = user_is_allowed(&app_state, user_uuid, id).await {
                                log::error!(
                                    "Failed to check if user '{}' is allowed to access charger '{}': {:?}",
                                    user_uuid,
                                    id,
                                    e
                                );
                                tokio::spawn(async move {
                                    let mut tun_sock = tunn_sock.lock().await;
                                    let nack_packet = ManagementPacket::NackPacket(
                                        NackPacket::new(NackReason::Unauthorized),
                                    );
                                    tun_sock.send_packet(nack_packet);
                                });
                                return;
                            }

                            let mut tun_sock = tunn_sock.lock().await;
                            if let Some(sender) = sender {
                                if sender.send(meta_data.data).is_err() {
                                    log::error!(
                                        "Failed to send charge log send trigger for charger '{}' to TCP socket",
                                        id
                                    );
                                    let nack_packet = ManagementPacket::NackPacket(
                                        NackPacket::new(NackReason::InternalError),
                                    );
                                    tun_sock.send_packet(nack_packet);
                                }
                            } else {
                                log::error!(
                                    "Failed to get sender for charge log send request for charger '{}'",
                                    id
                                );
                                let nack_packet = ManagementPacket::NackPacket(NackPacket::new(
                                    NackReason::InternalError,
                                ));
                                tun_sock.send_packet(nack_packet);
                            }
                            tun_sock.send_packet(ManagementPacket::AckPacket(AckPacket::new()));
                        }
                    }
                    // Charge log send request
                    PacketType::RequestChargeLogSend => {
                        // Check rate limit first
                        let device_id_str = id.to_string();
                        let ip_str = addr.ip().to_string();
                        if !bridge_state
                            .device_ratelimiter
                            .check_key(device_id_str, ip_str)
                        {
                            let mut tun_sock = tunn_sock.lock().await;
                            log::error!("Rate limit exceeded for charge log send request from charger with id '{}'", id);
                            let nack_packet = ManagementPacket::NackPacket(NackPacket::new(
                                NackReason::ToManyRequests,
                            ));
                            tun_sock.send_packet(nack_packet);
                            return;
                        }

                        let Ok(packet) = RequestChargeLogSendPacket::try_from(data.as_slice())
                        else {
                            log::error!("Failed to parse charge log send request packet from charger with id '{}'", id);
                            return;
                        };

                        let Ok(_guard) = CurrentChargeLogSendsRAII::new() else {
                            log::error!("Too many concurrent charge log sends, rejecting new request for charger with id '{}'", id);
                            let mut tun_sock = tunn_sock.lock().await;
                            let nack_packet =
                                ManagementPacket::NackPacket(NackPacket::new(NackReason::Busy));
                            tun_sock.send_packet(nack_packet);
                            return;
                        };
                        let Ok(last_charge_log_upload_hashes) =
                            get_last_charge_log_upload_hash(id, &app_state).await
                        else {
                            log::error!("Failed to get last charge log upload hash for charger with id '{}'", id);
                            let mut tun_sock = tunn_sock.lock().await;
                            let nack_packet = ManagementPacket::NackPacket(NackPacket::new(
                                NackReason::InternalError,
                            ));
                            tun_sock.send_packet(nack_packet);
                            return;
                        };

                        // last_charge_log_upload_hashes is now Vec<Option<Vec<u8>>>
                        let hash_exists = last_charge_log_upload_hashes.iter().any(|opt| {
                            if let Some(ref h) = opt {
                                h == &packet.hash.to_vec()
                            } else {
                                false
                            }
                        });
                        if hash_exists {
                            log::error!("Received charge log send request from charger with id '{}' with hash that matches a previously uploaded charge log, rejecting request", id);
                            let mut tun_sock = tunn_sock.lock().await;
                            let nack_packet = ManagementPacket::NackPacket(NackPacket::new(
                                NackReason::AlreadySent,
                            ));
                            tun_sock.send_packet(nack_packet);
                            return;
                        }

                        {
                            let mut tunn_sock = tunn_sock.lock().await;
                            if tunn_sock.has_sender() {
                                log::info!("Received charge log send request from charger with id '{}' while another send is still in progress", id);
                                let nack_packet = ManagementPacket::NackPacket(NackPacket::new(
                                    NackReason::OngoingRequest,
                                ));
                                tunn_sock.send_packet(nack_packet);
                                return;
                            }
                        }

                        let (sender, receiver) = tokio::sync::oneshot::channel();
                        {
                            let mut tun_sock = tunn_sock.lock().await;
                            tun_sock.set_sender(sender);
                        }

                        {
                            let mut tun_sock = tunn_sock.lock().await;
                            let ack_packet = ManagementPacket::AckPacket(AckPacket::new());
                            tun_sock.send_packet(ack_packet);
                        }

                        let meta_data = tokio::select! {
                            res = receiver => {
                                match res {
                                    Ok(meta_data) => meta_data,
                                    Err(e) => {
                                        log::error!("Failed to receive trigger for charge log send from charger with id '{}': {}", id, e);
                                        return;
                                    }
                                }
                            },
                            _ = tokio::time::sleep(std::time::Duration::from_secs(30)) => {
                                let mut tun_sock = tunn_sock.lock().await;
                                tun_sock.send_packet(ManagementPacket::NackPacket(NackPacket::new(NackReason::Timeout)));

                                log::error!("Did not receive trigger for charge log send from charger with id '{}' within timeout", id);
                                return;
                            }
                        };

                        let is_monthly_email = meta_data.is_monthly_email;
                        match handle_charge_log(meta_data, tunn_sock.clone(), app_state.clone())
                            .await
                        {
                            Ok(_) => {
                                if is_monthly_email {
                                    if let Err(e) = set_last_charge_log_upload_hash(
                                        id,
                                        packet.hash.to_vec(),
                                        &app_state,
                                    )
                                    .await
                                    {
                                        log::error!("Failed to set last charge log upload hash for charger with id '{}': {:?}", id, e);
                                    }
                                }
                                let mut tunn_sock = tunn_sock.lock().await;
                                let ack_packet = ManagementPacket::AckPacket(AckPacket::new());
                                tunn_sock.send_packet(ack_packet);
                            }
                            Err(e) => {
                                log::error!("Failed to handle charge log: {:?}", e);
                                let mut tunn_sock = tunn_sock.lock().await;
                                let nack_packet = ManagementPacket::NackPacket(NackPacket::new(
                                    NackReason::Timeout,
                                ));
                                tunn_sock.send_packet(nack_packet);
                            }
                        }
                    }
                    _ => {
                        log::error!("Received unknown management packet type {:02x} from charger with id '{}'", header.p_type as u8, id);
                    }
                }
            });
        } else {
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use boringtun::{
        noise::Tunn,
        x25519::{PublicKey, StaticSecret},
    };
    use std::{
        collections::{HashMap, HashSet},
        sync::atomic::Ordering,
        time::Instant,
    };

    fn peer_and_charger() -> (Tunn, Charger, Vec<u8>) {
        let server_private = StaticSecret::from([11u8; 32]);
        let peer_private = StaticSecret::from([22u8; 32]);
        let server_public = PublicKey::from(&server_private);
        let peer_public = PublicKey::from(&peer_private);
        let psk = [33u8; 32];
        let mut peer = Tunn::new(peer_private, server_public, Some(psk), None, 1, None);
        let mut output = [0u8; 2048];
        let TunnResult::WriteToNetwork(init) = peer.format_handshake_initiation(&mut output, false)
        else {
            panic!("expected handshake initiation");
        };
        let init = init.to_vec();
        let charger = Charger {
            id: uuid::Uuid::new_v4(),
            uid: 1,
            password: String::new(),
            name: None,
            management_private: BASE64_STANDARD.encode(server_private.to_bytes()),
            charger_pub: BASE64_STANDARD.encode(peer_public.as_bytes()),
            wg_charger_ip: "10.0.0.2/24".parse().unwrap(),
            psk: BASE64_STANDARD.encode(psk),
            wg_server_ip: "10.0.0.1/24".parse().unwrap(),
            webinterface_port: 80,
            firmware_version: String::new(),
            last_state_change: None,
            device_type: None,
            mtu: None,
            last_charge_log_upload_hash: vec![],
        };
        (peer, charger, init)
    }

    #[test]
    fn only_handshake_initiations_admit_unknown_peers() {
        let (_, _, init) = peer_and_charger();
        assert!(is_handshake_initiation(&init));
        assert!(!is_handshake_initiation(&init[..init.len() - 1]));
        let mut reserved_bits = init.clone();
        reserved_bits[1] = 1;
        assert!(!is_handshake_initiation(&reserved_bits));
        let mut transport = [0u8; 32];
        transport[0] = 4;
        assert!(!is_handshake_initiation(&transport));
        assert!(!is_handshake_initiation(&[]));
    }

    #[actix_web::test]
    async fn candidate_matching_establishes_a_working_wireguard_session() {
        let (mut peer, charger, init) = peer_and_charger();
        let mut wrong = charger.clone();
        wrong.management_private = BASE64_STANDARD.encode([99u8; 32]);
        let udp = Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap());
        let id = charger.id;
        let (matched_id, mut socket, response) = tokio::task::spawn_blocking(move || {
            match_tunnel(
                vec![wrong, charger],
                "127.0.0.1:12345".parse().unwrap(),
                &init,
                udp,
            )
        })
        .await
        .unwrap()
        .unwrap();
        assert_eq!(id, matched_id);
        let mut output = [0u8; 2048];
        let TunnResult::WriteToNetwork(keepalive) = peer.decapsulate(None, &response, &mut output)
        else {
            panic!("peer could not complete handshake");
        };
        assert!(socket.decrypt(keepalive).unwrap().is_empty());
    }

    #[actix_web::test]
    async fn server_replies_to_authenticated_wireguard_keepalives() {
        let server_udp = Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap());
        let client_udp = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let client_addr = client_udp.local_addr().unwrap();
        let (mut peer, charger, init) = peer_and_charger();
        let (_, mut socket, response) =
            match_tunnel(vec![charger], client_addr, &init, server_udp.clone()).unwrap();

        let mut output = [0u8; 2048];
        let TunnResult::WriteToNetwork(keepalive) = peer.decapsulate(None, &response, &mut output)
        else {
            panic!("peer could not complete handshake");
        };
        server_udp.writable().await.unwrap();
        socket.decrypt(keepalive).unwrap();

        let mut received = [0u8; 2048];
        let size = tokio::time::timeout(Duration::from_secs(1), client_udp.recv(&mut received))
            .await
            .expect("server did not reply to keepalive")
            .unwrap();
        assert!(matches!(
            peer.decapsulate(None, &received[..size], &mut output),
            TunnResult::Done
        ));
    }

    #[actix_web::test]
    async fn established_traffic_progresses_while_new_peer_database_lookups_stall() {
        use diesel_async::{
            pooled_connection::{AsyncDieselConnectionManager, ManagerConfig},
            AsyncPgConnection,
        };
        let lookups = Arc::new(AtomicUsize::new(0));
        let observed = lookups.clone();
        let mut config = ManagerConfig::<AsyncPgConnection>::default();
        config.custom_setup = Box::new(move |_| {
            observed.fetch_add(1, Ordering::SeqCst);
            Box::pin(std::future::pending())
        });
        let pool = db_connector::Pool::builder(AsyncDieselConnectionManager::new_with_config(
            "postgres://unused",
            config,
        ))
        .max_size(1)
        .build()
        .unwrap();
        let server_udp = Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap());
        let server_addr = server_udp.local_addr().unwrap();
        let client_udp = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let client_addr = client_udp.local_addr().unwrap();
        let (mut peer, charger, init) = peer_and_charger();
        let id = charger.id;
        let (_, socket, response) =
            match_tunnel(vec![charger], client_addr, &init, server_udp.clone()).unwrap();
        let socket = Arc::new(Mutex::new(socket));
        let bridge = web::Data::new(BridgeState {
            pool: pool.clone(),
            socket: server_udp,
            web_client_map: Mutex::new(HashMap::new()),
            undiscovered_clients: Mutex::new(HashMap::new()),
            device_management_map: Arc::new(Mutex::new(HashMap::from([(
                client_addr,
                socket.clone(),
            )]))),
            device_management_map_with_id: Arc::new(Mutex::new(HashMap::from([(
                id,
                socket.clone(),
            )]))),
            port_discovery: Arc::new(Mutex::new(HashMap::new())),
            device_remote_conn_map: Mutex::new(HashMap::new()),
            undiscovered_devices: Arc::new(Mutex::new(HashMap::from([(
                "127.0.0.1/32".parse().unwrap(),
                HashSet::from([crate::DiscoveryCharger {
                    id,
                    last_request: Instant::now(),
                }]),
            )]))),
            lost_connections: Mutex::new(HashMap::new()),
            state_update_clients: Mutex::new(HashMap::new()),
            device_ratelimiter: Arc::new(crate::rate_limit::ChargerRateLimiter::new()),
        });
        let state = web::Data::new(AppState {
            pool,
            jwt_secret: String::new(),
            mailer: None,
            frontend_url: String::new(),
            sender_email: String::new(),
            sender_name: String::new(),
            brand: Default::default(),
            keys_in_use: Mutex::new(HashSet::new()),
            hasher: Default::default(),
        });
        let server = actix_web::rt::spawn(run_server(
            bridge.clone(),
            state,
            Arc::new(GlobalSearchRateLimiter::new()),
        ));
        let mut reconnecting = Vec::new();
        for _ in 0..8 {
            let udp = UdpSocket::bind("127.0.0.1:0").await.unwrap();
            udp.send_to(&init, server_addr).await.unwrap();
            reconnecting.push(udp);
        }
        tokio::time::timeout(Duration::from_secs(2), async {
            while lookups.as_ref().load(Ordering::SeqCst) == 0 {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("new peer did not reach the stalled database");

        let mut output = [0u8; 2048];
        let TunnResult::WriteToNetwork(keepalive) = peer.decapsulate(None, &response, &mut output)
        else {
            panic!("peer could not complete handshake");
        };
        let sent_at = Instant::now();
        client_udp.send_to(keepalive, server_addr).await.unwrap();
        tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                if socket.lock().await.last_seen() < sent_at.elapsed() {
                    break;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("established device was blocked by new-peer discovery");
        assert!(bridge.device_management_map.try_lock().is_some());
        server.abort();
        let _ = server.await;
    }
}
