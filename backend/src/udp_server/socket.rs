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
    collections::VecDeque,
    future::{poll_fn, Future},
    net::{IpAddr, SocketAddr},
    sync::Arc,
    task::Poll,
    time::{Duration, Instant},
};
use tokio::net::UdpSocket;

use boringtun::noise::{rate_limiter::RateLimiter, Tunn, TunnResult};
use futures_util::lock::Mutex;
use ipnetwork::IpNetwork;
use smoltcp::{
    iface::{self, Config, Interface, SocketHandle, SocketSet},
    socket::{icmp, tcp, udp},
};

use crate::udp_server::packet::ChargeLogSendMetadata;

use super::{
    device::ManagementDevice,
    packet::ManagementPacket,
    pcap_logger::PcapLogger,
    peer_discovery::{random_management_address, PeerDiscovery, ALL_NODES_ADDRESS},
};

pub struct ManagementSocket<'a> {
    charger_id: uuid::Uuid,
    sock_handle: SocketHandle,
    sockets: SocketSet<'a>,
    iface: iface::Interface,
    device: ManagementDevice,
    rate_limiter: Arc<RateLimiter>,
    local_ip: IpAddr,
    peer_ip: IpAddr,
    peer_discovery: Option<PeerDiscovery>,
    registered_addresses: Option<(IpNetwork, IpNetwork)>,
    pending_confirmation: Option<u32>,
    pending_packets: VecDeque<Vec<u8>>,
    remote_addr: SocketAddr,
    udp_socket: Arc<UdpSocket>,
    last_seen: Instant,
    pub(super) discovery_generation: u64,
    out_sequence: u16,
    tcp_socket: Option<SocketHandle>,
    pcap_logger: PcapLogger,
    sender: Option<tokio::sync::oneshot::Sender<ChargeLogSendMetadata>>,
    /// Test-only sink for plaintext packets about to be handed off to the
    /// smoltcp UDP socket. `None` in production builds.
    #[cfg(test)]
    packet_capture: Option<Arc<std::sync::Mutex<Vec<Vec<u8>>>>>,
}

impl std::fmt::Debug for ManagementSocket<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ManagementSocket")
            .field("charger_id", &self.charger_id)
            .finish()
    }
}

impl<'a> ManagementSocket<'a> {
    pub fn new(
        self_ip: IpNetwork,
        peer_ip: IpNetwork,
        remote_addr: SocketAddr,
        tunn: Tunn,
        rate_limiter: Arc<RateLimiter>,
        udp_socket: Arc<UdpSocket>,
        charger_id: uuid::Uuid,
    ) -> Result<Self, String> {
        if self_ip.is_ipv4() != peer_ip.is_ipv4() {
            return Err("management tunnel addresses must have the same IP family".into());
        }
        let pcap_logger = PcapLogger::new();
        let mut device =
            ManagementDevice::new(udp_socket.clone(), tunn, remote_addr, pcap_logger.clone());

        let mut config = Config::new(smoltcp::wire::HardwareAddress::Ip);
        config.random_seed = rand::random();
        let mut interface = Interface::new(config, &mut device, smoltcp::time::Instant::now());
        interface.update_ip_addrs(|ip_addrs| {
            let _ = ip_addrs.push(smoltcp::wire::IpCidr::new(
                self_ip.ip().into(),
                self_ip.prefix(),
            ));
        });

        let rx_buf = udp::PacketBuffer::new(vec![udp::PacketMetadata::EMPTY; 32], vec![0; 65535]);
        let tx_buf = udp::PacketBuffer::new(vec![udp::PacketMetadata::EMPTY; 32], vec![0; 65535]);
        let socket = udp::Socket::new(rx_buf, tx_buf);

        let mut sockets = SocketSet::new(vec![]);
        let sock_handle = sockets.add(socket);
        let socket = sockets.get_mut::<udp::Socket>(sock_handle);

        // should never be an error value since the socket was just created and the port is never 0.
        socket.bind((self_ip.ip(), 12345)).unwrap();

        Ok(Self {
            charger_id,
            sock_handle,
            sockets,
            iface: interface,
            device,
            rate_limiter,
            local_ip: self_ip.ip(),
            peer_ip: peer_ip.ip(),
            peer_discovery: None,
            registered_addresses: None,
            pending_confirmation: None,
            pending_packets: VecDeque::new(),
            remote_addr,
            udp_socket,
            last_seen: Instant::now(),
            discovery_generation: 0,
            out_sequence: 1,
            tcp_socket: None,
            pcap_logger,
            sender: None,
            #[cfg(test)]
            packet_capture: None,
        })
    }

    /// Runtime management sessions choose a fresh random IPv6 address and
    /// discover the charger through ICMPv6. Registered service addresses remain
    /// a fallback for legacy firmware, independently of user tunnel keys.
    pub fn new_preferred_ipv6(
        registered_self: IpNetwork,
        registered_peer: IpNetwork,
        remote_addr: SocketAddr,
        tunn: Tunn,
        rate_limiter: Arc<RateLimiter>,
        udp_socket: Arc<UdpSocket>,
        charger_id: uuid::Uuid,
    ) -> Result<Self, String> {
        if registered_self.is_ipv4() != registered_peer.is_ipv4() {
            return Err("management tunnel addresses must have the same IP family".into());
        }
        let local_address = random_management_address();
        let local = IpNetwork::new(local_address.into(), 128).unwrap();
        let mut socket = Self::new(
            local,
            IpNetwork::new(ALL_NODES_ADDRESS.into(), 128).unwrap(),
            remote_addr,
            tunn,
            rate_limiter,
            udp_socket,
            charger_id,
        )?;
        socket.registered_addresses = Some((registered_self, registered_peer));
        socket.restart_peer_discovery();
        Ok(socket)
    }

    fn restart_peer_discovery(&mut self) {
        let Some((registered_self, registered_peer)) = self.registered_addresses else {
            return;
        };
        if let Some(discovery) = self.peer_discovery.take() {
            self.sockets.remove(discovery.handle);
        }
        // Preserve an established random server address across key rotation so
        // active TCP streams survive. A legacy fallback needs a new random ULA.
        let local = match self.local_ip {
            IpAddr::V6(local) if self.local_ip != registered_self.ip() => local,
            _ => random_management_address(),
        };
        self.local_ip = local.into();
        self.peer_ip = ALL_NODES_ADDRESS.into();
        self.iface.update_ip_addrs(|addrs| {
            addrs.clear();
            addrs
                .push(smoltcp::wire::IpCidr::new(local.into(), 128))
                .unwrap();
            // A second IPv6 source could invalidate probe checksum selection.
            if registered_self.is_ipv4() {
                addrs
                    .push(smoltcp::wire::IpCidr::new(
                        registered_self.ip().into(),
                        registered_self.prefix(),
                    ))
                    .unwrap();
            }
        });
        let udp = self.sockets.get_mut::<udp::Socket>(self.sock_handle);
        udp.close();
        udp.bind(12345).unwrap();
        let buffer =
            || icmp::PacketBuffer::new(vec![icmp::PacketMetadata::EMPTY; 4], vec![0; 1024]);
        let mut icmp = icmp::Socket::new(buffer(), buffer());
        PeerDiscovery::bind(&mut icmp);
        let handle = self.sockets.add(icmp);
        self.peer_discovery = Some(PeerDiscovery::new(
            handle,
            local,
            registered_self.ip(),
            registered_peer.ip(),
            Instant::now(),
        ));
        log::info!("Management tunnel {}: discovering IPv6 peer from {local} via {ALL_NODES_ADDRESS}; registered address {registered_peer}", self.charger_id);
    }

    pub(super) fn wait_for_handshake_confirmation(&mut self, response: &[u8]) {
        if matches!(
            Tunn::parse_incoming_packet(response),
            Ok(boringtun::noise::Packet::HandshakeResponse(_))
        ) {
            // The parser validated the response length; its sender index is
            // the receiver index of the confirming encrypted data packet.
            self.pending_confirmation =
                Some(u32::from_le_bytes(response[4..8].try_into().unwrap()));
        }
    }

    pub(super) fn is_discovering_peer(&self) -> bool {
        self.peer_discovery.is_some()
    }

    fn resolve_peer(&mut self, peer: IpAddr, confirmed: bool) {
        if let Some(discovery) = self.peer_discovery.take() {
            self.sockets.remove(discovery.handle);
            let local = if confirmed {
                self.local_ip
            } else {
                discovery.registered_local
            };
            self.local_ip = local;
            let prefix = if confirmed {
                128
            } else {
                self.registered_addresses
                    .map(|(registered_local, _)| registered_local.prefix())
                    .unwrap_or(if local.is_ipv4() { 32 } else { 128 })
            };
            // The registered IPv4 alias exists only while probing legacy peers.
            // After selection, UDP and TCP share the selected local address.
            self.iface.update_ip_addrs(|addrs| {
                addrs.clear();
                addrs
                    .push(smoltcp::wire::IpCidr::new(local.into(), prefix))
                    .unwrap();
            });
            let udp = self.sockets.get_mut::<udp::Socket>(self.sock_handle);
            udp.close();
            udp.bind((local, 12345)).unwrap();
        }
        self.peer_ip = peer;
        // Rekeying the same address preserves streams. A recreated charger
        // interface has lost its old TCP state and may use a different ULA.
        if let Some(socket) = self.get_tcp_socket() {
            if socket
                .remote_endpoint()
                .is_some_and(|remote| remote.addr != peer.into())
            {
                socket.abort();
            }
        }
        if confirmed {
            log::info!(
                "Management tunnel {}: selected peer {peer}",
                self.charger_id
            );
        } else {
            log::info!(
                "Management tunnel {}: IPv6 probe timed out; using peer {peer}",
                self.charger_id
            );
        }
    }

    fn flush_pending_packets(&mut self) {
        if self.peer_discovery.is_some() {
            return;
        }
        let socket = self.sockets.get_mut::<udp::Socket>(self.sock_handle);
        while let Some(data) = self.pending_packets.front() {
            if socket.send_slice(data, (self.peer_ip, 12345)).is_err() {
                break;
            }
            self.pending_packets.pop_front();
        }
    }

    pub fn reset(&mut self) {
        self.device.reset();
    }

    /// Construct a `ManagementSocket` suitable for unit/integration tests that
    /// only need a populated entry in `device_management_map_with_id`. The
    /// underlying boringtun state is initialized with zeroed keys; the socket
    /// is never expected to send or receive real traffic, so the lack of a
    /// real handshake partner is irrelevant.
    #[cfg(test)]
    pub async fn new_for_test(charger_id: uuid::Uuid, remote_addr: SocketAddr) -> Self {
        use boringtun::x25519::{PublicKey, StaticSecret};
        use std::net::Ipv4Addr;

        let static_private = StaticSecret::from([0u8; 32]);
        let peer_static_public = PublicKey::from([0u8; 32]);
        let rate_limiter = Arc::new(RateLimiter::new(&peer_static_public, 10));
        let tunn = Tunn::new(
            static_private,
            peer_static_public,
            None,
            Some(5),
            0,
            Some(rate_limiter.clone()),
        );
        let udp_socket = Arc::new(
            UdpSocket::bind("0.0.0.0:0")
                .await
                .expect("failed to bind test UDP socket"),
        );
        Self::new(
            IpNetwork::new(Ipv4Addr::UNSPECIFIED.into(), 24).unwrap(),
            IpNetwork::new(Ipv4Addr::UNSPECIFIED.into(), 24).unwrap(),
            remote_addr,
            tunn,
            rate_limiter,
            udp_socket,
            charger_id,
        )
        .unwrap()
    }

    /// Test helper: install a sink that records the plaintext bytes of every
    /// packet handed to `send_packet`. No-op outside of test builds.
    #[cfg(test)]
    pub fn enable_packet_capture(&mut self, capture: Arc<std::sync::Mutex<Vec<Vec<u8>>>>) {
        self.packet_capture = Some(capture);
    }

    pub fn poll(&mut self) {
        self.poll_at(Instant::now());
    }

    fn poll_at(&mut self, now: Instant) {
        // The responder keeps its old sending session until authenticated data
        // confirms the new receiver index. Probing earlier encrypts with stale
        // keys when the charger has just discarded its old tunnel instance.
        if self.pending_confirmation.is_some() {
            return;
        }
        self.iface.poll(
            smoltcp::time::Instant::now(),
            &mut self.device,
            &mut self.sockets,
        );
        if let Some(discovery) = self.peer_discovery.as_mut() {
            if let Some((peer, confirmed)) =
                discovery.poll(self.sockets.get_mut::<icmp::Socket>(discovery.handle), now)
            {
                self.resolve_peer(peer, confirmed);
            }
        }
        self.flush_pending_packets();
        self.iface.poll(
            smoltcp::time::Instant::now(),
            &mut self.device,
            &mut self.sockets,
        );
    }

    fn recv(&mut self) -> Option<Vec<u8>> {
        self.poll();
        loop {
            let (data, peer) = {
                let socket = self.sockets.get_mut::<udp::Socket>(self.sock_handle);
                let (data, endpoint) = socket.recv().ok()?;
                (data.to_vec(), IpAddr::from(endpoint.endpoint.addr))
            };
            if let Some(discovery) = &self.peer_discovery {
                if discovery.accepts(peer) {
                    // Legacy traffic may arrive during discovery, but only a
                    // correlated echo reply selects the random IPv6 peer.
                    return Some(data);
                }
            } else if peer == self.peer_ip {
                return Some(data);
            }
        }
    }

    pub fn reset_out_sequence(&mut self) {
        self.out_sequence = 1;
    }

    pub fn send_packet(&mut self, mut packet: ManagementPacket) {
        packet.set_seq_num(self.out_sequence);
        self.out_sequence += 1;

        let bytes = packet.as_bytes();
        #[cfg(test)]
        if let Some(capture) = self.packet_capture.as_ref() {
            if let Ok(mut buf) = capture.lock() {
                buf.push(bytes.clone());
            }
        }
        self.encrypt_and_send_slice(&bytes);
    }

    pub fn encrypt_and_send_slice(&mut self, data: &[u8]) {
        // Keep commands until address selection finishes. Bound storage in the
        // same way as the management UDP socket's 32-packet transmit queue.
        if self.pending_packets.len() < 32 {
            self.pending_packets.push_back(data.to_vec());
        } else {
            log::warn!("Management command queue is full for {}", self.charger_id);
        }
        self.poll();
    }

    fn send_slice(&self, data: &[u8]) -> Result<(), String> {
        match self.udp_socket.try_send_to(data, self.remote_addr) {
            Ok(sent) => {
                if sent != data.len() {
                    Err("Sent was incomplete".to_string())
                } else {
                    Ok(())
                }
            }
            Err(err) => Err(err.to_string()),
        }
    }

    fn send_keepalive(&mut self) -> Result<(), String> {
        // An empty WireGuard payload is a 32-byte transport packet. Keep the
        // buffer at BoringTun's documented minimum size for encapsulation.
        let mut dst = [0u8; 148];
        match self.device.encapsulate(&[], &mut dst) {
            TunnResult::WriteToNetwork(data) => self.send_slice(data),
            TunnResult::Done => Ok(()),
            TunnResult::Err(err) => Err(format!("{err:?}")),
            TunnResult::WriteToTunnelV4(_, _) | TunnResult::WriteToTunnelV6(_, _) => {
                Err("encapsulating a keepalive returned a tunnel packet".to_string())
            }
        }
    }

    pub fn decrypt(&mut self, data: &[u8]) -> Result<Vec<u8>, String> {
        let receiver_index = match Tunn::parse_incoming_packet(data) {
            Ok(boringtun::noise::Packet::PacketData(packet)) => Some(packet.receiver_idx),
            _ => None,
        };
        let is_keepalive = receiver_index.is_some();
        let is_handshake = matches!(
            boringtun::noise::Tunn::parse_incoming_packet(data),
            Ok(boringtun::noise::Packet::HandshakeInit(_))
        );
        // Draining a handshake may emit queued IP packets larger than the
        // handshake datagram. Reserve room for a full IP packet plus WG framing.
        let mut dst = vec![0; (u16::MAX as usize + 32).max(data.len())];
        match self.device.decapsulate(data, &mut dst) {
            TunnResult::WriteToNetwork(data) => {
                let authenticated_handshake = if is_handshake {
                    match Tunn::parse_incoming_packet(data) {
                        Ok(boringtun::noise::Packet::HandshakeResponse(_)) => {
                            Some(u32::from_le_bytes(data[4..8].try_into().unwrap()))
                        }
                        _ => None,
                    }
                } else {
                    None
                };
                self.send_slice(data)?;
                while let TunnResult::WriteToNetwork(data) =
                    self.device.decapsulate(&Vec::new(), &mut dst)
                {
                    self.send_slice(data)?;
                }
                if let Some(index) = authenticated_handshake {
                    if self.registered_addresses.is_some() {
                        // A new charger may reuse its public key and UDP
                        // endpoint while discarding its previous session keys.
                        self.restart_peer_discovery();
                        self.pending_confirmation = Some(index);
                    }
                }
                self.last_seen = Instant::now();
                Ok(Vec::new())
            }
            TunnResult::WriteToTunnelV4(data, _) | TunnResult::WriteToTunnelV6(data, _) => {
                if receiver_index == self.pending_confirmation {
                    self.pending_confirmation = None;
                }
                // smoltcp accepts all-nodes multicast even without a local
                // IPv6 address. Do not let an IPv4-only profile emit replies
                // from ::1 to discovery probes or stale IPv6 traffic.
                let version = data.first().map(|byte| byte >> 4);
                let family_configured = self.iface.ip_addrs().iter().any(|address| {
                    matches!(
                        (version, address.address()),
                        (Some(4), smoltcp::wire::IpAddress::Ipv4(_))
                            | (Some(6), smoltcp::wire::IpAddress::Ipv6(_))
                    )
                });
                self.last_seen = Instant::now();
                if !family_configured {
                    return Ok(Vec::new());
                }
                self.device.push_packet(data.to_owned());
                self.poll();
                self.last_seen = Instant::now();
                if let Some(data) = self.recv() {
                    Ok(data)
                } else {
                    Ok(Vec::new())
                }
            }
            TunnResult::Err(err) => Err(format!("{err:?}")),
            TunnResult::Done => {
                if receiver_index.is_some() && receiver_index == self.pending_confirmation {
                    self.pending_confirmation = None;
                }
                // BoringTun reports an authenticated empty transport packet as
                // `Done`. Reply so a client using persistent keepalives can
                // confirm that the management peer remains reachable.
                if is_keepalive {
                    self.send_keepalive()?;
                }
                self.last_seen = Instant::now();
                Ok(Vec::new())
            }
        }
    }

    pub fn reset_rate_limiter(&self) {
        self.rate_limiter.reset_count()
    }

    #[cfg(test)]
    pub(super) fn expire_for_test(&mut self) {
        self.last_seen = Instant::now() - Duration::from_secs(60);
    }

    pub fn last_seen(&self) -> Duration {
        self.last_seen.elapsed()
    }

    pub fn id(&self) -> uuid::Uuid {
        self.charger_id
    }

    pub fn get_remote_address(&self) -> SocketAddr {
        self.remote_addr
    }

    pub fn get_tcp_socket(&mut self) -> Option<&mut tcp::Socket<'a>> {
        if let Some(handle) = self.tcp_socket {
            return Some(self.sockets.get_mut(handle));
        }

        None
    }

    pub fn init_tcp_socket(&mut self) {
        let rx_buf = tcp::SocketBuffer::new(vec![0; 65535]);
        let tx_buf = tcp::SocketBuffer::new(vec![0; 65535]);
        let socket = tcp::Socket::new(rx_buf, tx_buf);
        let handle = self.sockets.add(socket);
        self.tcp_socket = Some(handle);
    }

    pub fn remove_tcp_socket(&mut self) {
        if let Some(handle) = self.tcp_socket.take() {
            self.sockets.remove(handle);
        }
    }

    /// Enables pcap logging for this socket and writes to the specified file path.
    pub fn enable_pcap_logging(&self, path: std::path::PathBuf) -> Result<(), String> {
        self.pcap_logger.enable(path)
    }

    /// Disables pcap logging for this socket.
    pub fn disable_pcap_logging(&self) {
        self.pcap_logger.disable();
    }

    /// Returns whether pcap logging is enabled for this socket.
    pub fn is_pcap_logging_enabled(&self) -> bool {
        self.pcap_logger.is_enabled()
    }

    /// Returns the current pcap file path if logging is enabled.
    pub fn get_pcap_file_path(&self) -> Option<std::path::PathBuf> {
        self.pcap_logger.get_file_path()
    }

    pub fn set_sender(&mut self, sender: tokio::sync::oneshot::Sender<ChargeLogSendMetadata>) {
        self.sender = Some(sender);
    }

    pub fn take_sender(&mut self) -> Option<tokio::sync::oneshot::Sender<ChargeLogSendMetadata>> {
        self.sender.take()
    }

    pub fn has_sender(&self) -> bool {
        self.sender.is_some()
    }
}

pub enum TCPRecvResult {
    Ok(Vec<u8>),
    Finished,
    Err(std::io::Error),
}

pub struct ManagementSocketTCPReceiver<'a>(Arc<Mutex<ManagementSocket<'a>>>);

impl<'a> ManagementSocketTCPReceiver<'a> {
    pub async fn new(socket: Arc<Mutex<ManagementSocket<'a>>>) -> Self {
        let mut sock_lock = socket.lock().await;
        sock_lock.init_tcp_socket();
        let tcp_socket = sock_lock.get_tcp_socket().unwrap();
        tcp_socket.listen(8080).unwrap();
        drop(sock_lock);

        Self(socket)
    }

    pub async fn handle_tcp_recv(&self) -> TCPRecvResult {
        // Keep the lock future between polls so contention registers a wakeup
        // on unlock instead of scheduling a busy retry loop.
        let mut lock = Box::pin(self.0.lock());
        poll_fn(|ctx| {
            let mut tunn_sock = std::task::ready!(lock.as_mut().poll(ctx));
            lock = Box::pin(self.0.lock());

            if let Some(socket) = tunn_sock.get_tcp_socket() {
                socket.register_recv_waker(ctx.waker());

                if socket.state() == tcp::State::Closed {
                    return Poll::Ready(TCPRecvResult::Err(std::io::Error::new(
                        std::io::ErrorKind::ConnectionReset,
                        "Management TCP socket closed",
                    )));
                }

                if let tcp::State::CloseWait = socket.state() {
                    if socket.recv_queue() != 0 {
                        match socket.recv(|buf| (buf.len(), buf.to_vec())) {
                            Ok(data) => {
                                return Poll::Ready(TCPRecvResult::Ok(data));
                            }
                            Err(tcp::RecvError::Finished) => {
                                return Poll::Ready(TCPRecvResult::Err(std::io::Error::new(
                                    std::io::ErrorKind::UnexpectedEof,
                                    "TCP socket finished",
                                )));
                            }
                            Err(tcp::RecvError::InvalidState) => {
                                log::error!("TCP socket in invalid state");
                                return Poll::Pending;
                            }
                        }
                    }
                    socket.close();
                    return Poll::Ready(TCPRecvResult::Finished);
                }

                if !socket.can_recv() {
                    return Poll::Pending;
                }

                match socket.recv(|buf| (buf.len(), buf.to_vec())) {
                    Ok(data) => Poll::Ready(TCPRecvResult::Ok(data)),
                    Err(tcp::RecvError::Finished) => {
                        Poll::Ready(TCPRecvResult::Err(std::io::Error::new(
                            std::io::ErrorKind::UnexpectedEof,
                            "TCP socket finished",
                        )))
                    }
                    Err(tcp::RecvError::InvalidState) => {
                        log::error!("TCP socket in invalid state");
                        Poll::Pending
                    }
                }
            } else {
                Poll::Ready(TCPRecvResult::Err(std::io::Error::new(
                    std::io::ErrorKind::NotConnected,
                    "TCP socket not initialized",
                )))
            }
        })
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::super::peer_discovery::{MANAGEMENT_CHARGER_ADDRESS, MANAGEMENT_SERVER_ADDRESS};
    use super::*;
    use futures_util::task::{waker_ref, ArcWake};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::task::Context;

    async fn management_pair(
        server_cidr: &str,
        charger_cidr: &str,
    ) -> (ManagementSocket<'static>, ManagementSocket<'static>) {
        management_pair_with_registration(server_cidr, charger_cidr, None).await
    }

    async fn management_pair_with_registration(
        server_cidr: &str,
        charger_cidr: &str,
        registered: Option<(&str, &str)>,
    ) -> (ManagementSocket<'static>, ManagementSocket<'static>) {
        use boringtun::x25519::{PublicKey, StaticSecret};
        let server_key = StaticSecret::from([11; 32]);
        let charger_key = StaticSecret::from([22; 32]);
        let server_pub = PublicKey::from(&server_key);
        let charger_pub = PublicKey::from(&charger_key);
        let server_limiter = Arc::new(RateLimiter::new(&server_pub, 100));
        let charger_limiter = Arc::new(RateLimiter::new(&charger_pub, 100));
        let mut server_tun = Tunn::new(
            server_key,
            charger_pub,
            Some([33; 32]),
            None,
            1,
            Some(server_limiter.clone()),
        );
        let mut charger_tun = Tunn::new(
            charger_key,
            server_pub,
            Some([33; 32]),
            None,
            2,
            Some(charger_limiter.clone()),
        );
        let mut output = [0; 2048];
        let TunnResult::WriteToNetwork(init) =
            charger_tun.format_handshake_initiation(&mut output, false)
        else {
            panic!()
        };
        let init = init.to_vec();
        let TunnResult::WriteToNetwork(response) = server_tun.decapsulate(None, &init, &mut output)
        else {
            panic!()
        };
        let response = response.to_vec();
        let TunnResult::WriteToNetwork(keepalive) =
            charger_tun.decapsulate(None, &response, &mut output)
        else {
            panic!()
        };
        let keepalive = keepalive.to_vec();
        assert!(matches!(
            server_tun.decapsulate(None, &keepalive, &mut output),
            TunnResult::Done
        ));

        // These outer sockets remain IPv4 even when the inner tunnel is IPv6.
        let server_udp = Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap());
        let charger_udp = Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap());
        server_udp.writable().await.unwrap();
        charger_udp.writable().await.unwrap();
        let server = if let Some((local, peer)) = registered {
            ManagementSocket::new_preferred_ipv6(
                local.parse().unwrap(),
                peer.parse().unwrap(),
                charger_udp.local_addr().unwrap(),
                server_tun,
                server_limiter,
                server_udp.clone(),
                uuid::Uuid::new_v4(),
            )
        } else {
            ManagementSocket::new(
                server_cidr.parse().unwrap(),
                charger_cidr.parse().unwrap(),
                charger_udp.local_addr().unwrap(),
                server_tun,
                server_limiter,
                server_udp.clone(),
                uuid::Uuid::new_v4(),
            )
        }
        .unwrap();
        let charger = ManagementSocket::new(
            charger_cidr.parse().unwrap(),
            server_cidr.parse().unwrap(),
            server_udp.local_addr().unwrap(),
            charger_tun,
            charger_limiter,
            charger_udp,
            uuid::Uuid::new_v4(),
        )
        .unwrap();
        (server, charger)
    }

    // Model the charger's multicast echo handler, which always replies from
    // the random service ULA used by management UDP and TCP.
    async fn answer_discovery_probe(charger: &mut ManagementSocket<'_>) {
        use smoltcp::wire::{Icmpv6Packet, Icmpv6Repr, IpProtocol, Ipv6Packet, Ipv6Repr};
        let mut network = [0; 2048];
        let size = tokio::time::timeout(
            Duration::from_secs(1),
            charger.udp_socket.recv(&mut network),
        )
        .await
        .unwrap()
        .unwrap();
        let mut output = [0; 2048];
        let TunnResult::WriteToTunnelV6(bytes, _) =
            charger.device.decapsulate(&network[..size], &mut output)
        else {
            panic!("expected discovery probe")
        };
        let packet = Ipv6Packet::new_checked(&*bytes).unwrap();
        assert_eq!(packet.dst_addr(), ALL_NODES_ADDRESS);
        assert_eq!(packet.hop_limit(), 1);
        assert_eq!(packet.next_header(), IpProtocol::Icmpv6);
        let server = packet.src_addr();
        let IpAddr::V6(local) = charger.local_ip else {
            panic!()
        };
        let Icmpv6Repr::EchoRequest {
            ident,
            seq_no,
            data,
        } = Icmpv6Repr::parse(
            &server,
            &packet.dst_addr(),
            &Icmpv6Packet::new_checked(packet.payload()).unwrap(),
            &Default::default(),
        )
        .unwrap()
        else {
            panic!()
        };
        assert_eq!(ident, 0x4d47);
        let reply = Icmpv6Repr::EchoReply {
            ident,
            seq_no,
            data,
        };
        let mut bytes = vec![0; 40 + reply.buffer_len()];
        let mut packet = Ipv6Packet::new_unchecked(&mut bytes);
        Ipv6Repr {
            src_addr: local,
            dst_addr: server,
            next_header: IpProtocol::Icmpv6,
            payload_len: reply.buffer_len(),
            hop_limit: 64,
        }
        .emit(&mut packet);
        reply.emit(
            &local,
            &server,
            &mut Icmpv6Packet::new_unchecked(packet.payload_mut()),
            &Default::default(),
        );
        charger.peer_ip = server.into();
        let mut encrypted = [0; 2048];
        let TunnResult::WriteToNetwork(bytes) = charger.device.encapsulate(&bytes, &mut encrypted)
        else {
            panic!()
        };
        charger.send_slice(bytes).unwrap();
    }

    async fn deliver_one(to: &mut ManagementSocket<'_>) -> Vec<u8> {
        let mut packet = [0; 2048];
        let size = tokio::time::timeout(Duration::from_secs(1), to.udp_socket.recv(&mut packet))
            .await
            .expect("management peer did not receive encrypted UDP")
            .unwrap();
        to.decrypt(&packet[..size]).unwrap()
    }

    async fn pump(to: &mut ManagementSocket<'_>) {
        let mut packet = [0; 2048];
        for _ in 0..32 {
            match tokio::time::timeout(Duration::from_millis(5), to.udp_socket.recv(&mut packet))
                .await
            {
                Ok(Ok(size)) => {
                    to.decrypt(&packet[..size]).unwrap();
                }
                Ok(Err(error)) => panic!("UDP receive failed: {error}"),
                Err(_) => return,
            }
        }
        panic!("management peer kept sending unexpectedly");
    }

    #[actix_web::test]
    async fn management_udp_preserves_prefixes_and_supports_both_inner_families() {
        for (local, peer) in [
            ("10.123.123.3", "10.123.123.2"),
            ("10.123.123.3/24", "10.123.123.2/24"),
            ("fd00:123:ffff::3/64", "fd00:123:ffff::2/64"),
            ("fd00:123:ffff::3", "fd00:123:ffff::2"),
        ] {
            let (mut server, mut charger) = management_pair(local, peer).await;
            let registered: IpNetwork = local.parse().unwrap();
            assert_eq!(server.iface.ip_addrs()[0].address(), registered.ip().into());
            assert_eq!(server.iface.ip_addrs()[0].prefix_len(), registered.prefix());
            charger.encrypt_and_send_slice(b"management request");
            assert_eq!(deliver_one(&mut server).await, b"management request");
            server.encrypt_and_send_slice(b"management response");
            assert_eq!(deliver_one(&mut charger).await, b"management response");
        }
    }

    #[actix_web::test]
    async fn management_tcp_charge_log_works_with_ipv4_and_requested_ipv6_addresses() {
        for (local, peer) in [
            ("10.123.123.3", "10.123.123.2"),
            ("fd00:123:ffff::3/64", "fd00:123:ffff::2/64"),
            ("fd00:123:ffff::3/128", "fd00:123:ffff::2/128"),
        ] {
            let (mut server, mut charger) = management_pair(local, peer).await;
            server.init_tcp_socket();
            let server_ip = server.iface.ip_addrs()[0].address();
            server
                .get_tcp_socket()
                .unwrap()
                .listen((server_ip, 8080))
                .unwrap();
            charger.init_tcp_socket();
            let handle = charger.tcp_socket.unwrap();
            charger
                .sockets
                .get_mut::<tcp::Socket>(handle)
                .connect(charger.iface.context(), (server_ip, 8080), 49152)
                .unwrap();
            charger.poll();
            for _ in 0..10 {
                pump(&mut server).await;
                pump(&mut charger).await;
                if charger.get_tcp_socket().unwrap().can_send() {
                    break;
                }
            }
            assert!(
                charger.get_tcp_socket().unwrap().can_send(),
                "TCP handshake failed for {local}"
            );
            charger
                .get_tcp_socket()
                .unwrap()
                .send_slice(b"charge log over management tunnel")
                .unwrap();
            charger.poll();
            for _ in 0..10 {
                pump(&mut server).await;
                pump(&mut charger).await;
                if server.get_tcp_socket().unwrap().can_recv() {
                    break;
                }
            }
            let received = server
                .get_tcp_socket()
                .unwrap()
                .recv(|bytes| (bytes.len(), bytes.to_vec()))
                .unwrap();
            assert_eq!(received, b"charge log over management tunnel");
        }
    }

    #[actix_web::test]
    async fn ipv6_management_peer_answers_echo_at_its_registered_address() {
        use smoltcp::{
            socket::icmp,
            wire::{Icmpv6Packet, Icmpv6Repr},
        };
        let (mut server, mut charger) =
            management_pair("fd00:123:ffff::3/128", "fd00:123:ffff::2/128").await;
        let local = "fd00:123:ffff::2".parse().unwrap();
        let peer = "fd00:123:ffff::3".parse().unwrap();
        let buffer =
            || icmp::PacketBuffer::new(vec![icmp::PacketMetadata::EMPTY; 4], vec![0; 1024]);
        let mut icmp = icmp::Socket::new(buffer(), buffer());
        icmp.bind(icmp::Endpoint::Ident(0x5757)).unwrap();
        let repr = Icmpv6Repr::EchoRequest {
            ident: 0x5757,
            seq_no: 17,
            data: b"management-peer-discovery",
        };
        let payload = icmp
            .send(repr.buffer_len(), smoltcp::wire::IpAddress::Ipv6(peer))
            .unwrap();
        repr.emit(
            &local,
            &peer,
            &mut Icmpv6Packet::new_unchecked(payload),
            &Default::default(),
        );
        let handle = charger.sockets.add(icmp);
        charger.poll();
        pump(&mut server).await;
        pump(&mut charger).await;
        let (bytes, address) = charger
            .sockets
            .get_mut::<icmp::Socket>(handle)
            .recv()
            .unwrap();
        assert_eq!(address, smoltcp::wire::IpAddress::Ipv6(peer));
        let packet = Icmpv6Packet::new_checked(bytes).unwrap();
        assert_eq!(
            Icmpv6Repr::parse(&peer, &local, &packet, &Default::default()).unwrap(),
            Icmpv6Repr::EchoReply {
                ident: 0x5757,
                seq_no: 17,
                data: b"management-peer-discovery"
            }
        );
    }

    #[actix_web::test]
    async fn management_socket_rejects_mixed_inner_families() {
        let (server, _) = management_pair("10.123.123.3", "10.123.123.2").await;
        let key = boringtun::x25519::StaticSecret::from([7; 32]);
        let peer = boringtun::x25519::PublicKey::from([8; 32]);
        let result = ManagementSocket::new(
            "fd00:123:ffff::3/64".parse().unwrap(),
            "10.123.123.2".parse().unwrap(),
            server.remote_addr,
            Tunn::new(key, peer, None, None, 0, None),
            server.rate_limiter,
            server.udp_socket,
            server.charger_id,
        );
        assert!(result.unwrap_err().contains("same IP family"));
    }

    #[actix_web::test]
    async fn stale_ipv4_registration_discovers_ipv6_and_flushes_queued_commands() {
        let (mut server, mut charger) = management_pair_with_registration(
            "fd00:123:ffff::3/128",
            "fd00:123:ffff::2/128",
            Some(("10.123.123.3", "10.123.123.2")),
        )
        .await;
        let IpAddr::V6(local) = server.local_ip else {
            panic!()
        };
        assert_eq!(local.octets()[0], 0xfd);
        assert_ne!(local, MANAGEMENT_SERVER_ADDRESS);
        assert_eq!(server.peer_ip, IpAddr::V6(ALL_NODES_ADDRESS));
        server.encrypt_and_send_slice(b"command using preferred IPv6");
        assert_eq!(server.pending_packets.len(), 1);
        // The first encrypted packet is ICMPv6; the charger replies from its
        // service address to the random relay address despite stale registration.
        answer_discovery_probe(&mut charger).await;
        assert!(deliver_one(&mut server).await.is_empty());
        assert!(!server.is_discovering_peer());
        assert_eq!(server.peer_ip, IpAddr::V6(MANAGEMENT_CHARGER_ADDRESS));
        assert!(server.pending_packets.is_empty());
        assert_eq!(
            deliver_one(&mut charger).await,
            b"command using preferred IPv6"
        );
        charger.encrypt_and_send_slice(b"IPv6 management response");
        assert_eq!(deliver_one(&mut server).await, b"IPv6 management response");
    }

    #[actix_web::test]
    async fn legacy_device_gets_queued_commands_after_bounded_ipv6_probe() {
        let (mut server, mut charger) = management_pair_with_registration(
            "10.123.123.3",
            "10.123.123.2",
            Some(("10.123.123.3", "10.123.123.2")),
        )
        .await;
        server.encrypt_and_send_slice(b"legacy command after timeout");
        assert_eq!(server.peer_ip, IpAddr::V6(ALL_NODES_ADDRESS));
        assert!(deliver_one(&mut charger).await.is_empty());
        assert!(server.is_discovering_peer());
        server.poll_at(
            Instant::now()
                + super::super::peer_discovery::DISCOVERY_TIMEOUT
                + Duration::from_millis(1),
        );
        assert!(!server.is_discovering_peer());
        assert_eq!(server.peer_ip, "10.123.123.2".parse::<IpAddr>().unwrap());
        assert_eq!(
            deliver_one(&mut charger).await,
            b"legacy command after timeout"
        );
        charger.encrypt_and_send_slice(b"legacy management response");
        assert_eq!(
            deliver_one(&mut server).await,
            b"legacy management response"
        );
    }

    #[actix_web::test]
    async fn legacy_management_traffic_does_not_override_pending_ipv6_preference() {
        let (mut server, mut charger) = management_pair_with_registration(
            "10.123.123.3",
            "10.123.123.2",
            Some(("10.123.123.3", "10.123.123.2")),
        )
        .await;
        charger.encrypt_and_send_slice(b"legacy device is already sending");
        assert_eq!(
            deliver_one(&mut server).await,
            b"legacy device is already sending"
        );
        assert!(server.is_discovering_peer());
        assert_eq!(server.peer_ip, IpAddr::V6(ALL_NODES_ADDRESS));
    }

    #[actix_web::test]
    async fn maintenance_drives_discovery_without_management_commands() {
        use std::collections::HashMap;
        let (server, mut charger) = management_pair_with_registration(
            "fd00:123:ffff::3/128",
            "fd00:123:ffff::2/128",
            Some(("10.123.123.3", "10.123.123.2")),
        )
        .await;
        let id = server.id();
        let server = Arc::new(Mutex::new(server));
        let sockets = Arc::new(Mutex::new(HashMap::from([(id, server.clone())])));
        super::super::poll_management_discovery_once(&sockets).await;
        answer_discovery_probe(&mut charger).await;
        assert!(deliver_one(&mut *server.lock().await).await.is_empty());
        assert!(!server.lock().await.is_discovering_peer());
        assert_eq!(
            server.lock().await.peer_ip,
            IpAddr::V6(MANAGEMENT_CHARGER_ADDRESS)
        );
    }

    struct WakeCounter(AtomicUsize);
    impl ArcWake for WakeCounter {
        fn wake_by_ref(this: &Arc<Self>) {
            this.0.fetch_add(1, Ordering::SeqCst);
        }
    }

    #[actix_web::test]
    async fn tcp_receiver_waits_for_unlock_without_self_waking() {
        let socket = Arc::new(Mutex::new(
            ManagementSocket::new_for_test(
                uuid::Uuid::new_v4(),
                "127.0.0.1:12345".parse().unwrap(),
            )
            .await,
        ));
        let receiver = ManagementSocketTCPReceiver::new(socket.clone()).await;
        let guard = socket.lock().await;
        let mut receive = Box::pin(receiver.handle_tcp_recv());
        let counter = Arc::new(WakeCounter(AtomicUsize::new(0)));
        let waker = waker_ref(&counter);
        let mut cx = Context::from_waker(&waker);
        assert!(receive.as_mut().poll(&mut cx).is_pending());
        assert_eq!(
            counter.0.load(Ordering::SeqCst),
            0,
            "receiver busy-wakes while locked"
        );
        drop(guard);
        assert!(
            counter.0.load(Ordering::SeqCst) > 0,
            "unlock did not wake receiver"
        );
        assert!(receive.as_mut().poll(&mut cx).is_pending());
        // Waiting for incoming TCP data must release the mutex for decrypt().
        socket
            .try_lock()
            .expect("receiver held lock while waiting for TCP")
            .remove_tcp_socket();
        assert!(matches!(
            receive.as_mut().poll(&mut cx),
            Poll::Ready(TCPRecvResult::Err(_))
        ));
    }

    include!("random_management_tests.rs");
}
