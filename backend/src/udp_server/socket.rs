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

use std::{future::poll_fn, net::{Ipv4Addr, SocketAddr}, sync::Arc, task::Poll, time::{Duration, Instant}};
use tokio::net::UdpSocket;

use boringtun::noise::{rate_limiter::RateLimiter, Tunn, TunnResult};
use futures_util::lock::Mutex;
use smoltcp::{
    iface::{self, Config, Interface, SocketHandle, SocketSet},
    socket::{tcp, udp},
};

use crate::udp_server::packet::ChargeLogSendMetadata;

use super::{device::ManagementDevice, packet::ManagementPacket, pcap_logger::PcapLogger};

pub struct ManagementSocket<'a> {
    charger_id: uuid::Uuid,
    sock_handle: SocketHandle,
    sockets: SocketSet<'a>,
    iface: iface::Interface,
    device: ManagementDevice,
    rate_limiter: Arc<RateLimiter>,
    peer_ip: Ipv4Addr,
    remote_addr: SocketAddr,
    udp_socket: Arc<UdpSocket>,
    last_seen: Instant,
    out_sequence: u16,
    tcp_socket: Option<SocketHandle>,
    pcap_logger: PcapLogger,
    sender: Option<tokio::sync::oneshot::Sender<ChargeLogSendMetadata>>,
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
        self_ip: Ipv4Addr,
        peer_ip: Ipv4Addr,
        remote_addr: SocketAddr,
        tunn: Tunn,
        rate_limiter: Arc<RateLimiter>,
        udp_socket: Arc<UdpSocket>,
        charger_id: uuid::Uuid,
    ) -> Self {
        let pcap_logger = PcapLogger::new();
        let mut device = ManagementDevice::new(udp_socket.clone(), tunn, remote_addr, pcap_logger.clone());

        let mut config = Config::new(smoltcp::wire::HardwareAddress::Ip);
        config.random_seed = rand::random();
        let mut interface = Interface::new(config, &mut device, smoltcp::time::Instant::now());
        interface.update_ip_addrs(|ip_addrs| {
            log::debug!("listening on ip: {self_ip}");
            let _ = ip_addrs.push(smoltcp::wire::IpCidr::new(
                smoltcp::wire::IpAddress::Ipv4(self_ip),
                24,
            ));
        });

        let rx_buf = udp::PacketBuffer::new(vec![udp::PacketMetadata::EMPTY; 32], vec![0; 65535]);
        let tx_buf = udp::PacketBuffer::new(vec![udp::PacketMetadata::EMPTY; 32], vec![0; 65535]);
        let socket = udp::Socket::new(rx_buf, tx_buf);

        let mut sockets = SocketSet::new(vec![]);
        let sock_handle = sockets.add(socket);
        let socket = sockets.get_mut::<udp::Socket>(sock_handle);

        // should never be an error value since the socket was just created and the port is never 0.
        socket.bind(12345).unwrap();

        Self {
            charger_id,
            sock_handle,
            sockets,
            iface: interface,
            device,
            rate_limiter,
            peer_ip,
            remote_addr,
            udp_socket,
            last_seen: Instant::now(),
            out_sequence: 1,
            tcp_socket: None,
            pcap_logger,
            sender: None,
        }
    }

    pub fn reset(&mut self) {
        self.device.reset();
    }

    pub fn poll(&mut self) {
        let now = smoltcp::time::Instant::now();
        self.iface.poll(now, &mut self.device, &mut self.sockets);
    }

    fn recv(&mut self) -> Option<Vec<u8>> {
        self.poll();
        let socket = self.sockets.get_mut::<udp::Socket>(self.sock_handle);
        if socket.can_recv() {
            match socket.recv() {
                Ok((data, endpoint)) => {
                    if endpoint.endpoint.addr != self.peer_ip.into() {
                        return None;
                    }

                    Some(data.to_vec())
                }
                Err(_) => None,
            }
        } else {
            None
        }
    }

    pub fn reset_out_sequence(&mut self) {
        self.out_sequence = 1;
    }

    pub fn send_packet(&mut self, mut packet: ManagementPacket) {
        packet.set_seq_num(self.out_sequence);
        self.out_sequence += 1;
        self.encrypt_and_send_slice(packet.as_bytes());
    }

    pub fn encrypt_and_send_slice(&mut self, data: &[u8]) {
        let socket = self.sockets.get_mut::<udp::Socket>(self.sock_handle);
        if socket.can_send() {
            let _ = socket.send_slice(data, (self.peer_ip, 12345));
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

    pub fn decrypt(&mut self, data: &[u8]) -> Result<Vec<u8>, String> {
        let mut dst = vec![0; data.len()];
        match self.device.decapsulate(data, &mut dst) {
            TunnResult::WriteToNetwork(data) => {
                self.send_slice(data)?;
                while let TunnResult::WriteToNetwork(data) =
                    self.device.decapsulate(&Vec::new(), &mut dst)
                {
                    self.send_slice(data)?;
                }
                self.last_seen = Instant::now();
                Ok(Vec::new())
            }
            TunnResult::WriteToTunnelV4(data, _) => {
                self.device.push_packet(data.to_owned());
                self.poll();
                self.last_seen = Instant::now();
                if let Some(data) = self.recv() {
                    Ok(data)
                } else {
                    Ok(Vec::new())
                }
            }
            TunnResult::WriteToTunnelV6(_, _) => {
                Err("Received an decryptable IPv6 packet, what is going on here?".to_string())
            }
            TunnResult::Err(err) => Err(format!("{err:?}")),
            TunnResult::Done => {
                self.last_seen = Instant::now();
                Ok(Vec::new())
            }
        }
    }

    pub fn reset_rate_limiter(&self) {
        self.rate_limiter.reset_count()
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

    pub fn init_tcp_socket(&mut self) -> () {
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

    pub fn take_sender(&mut self,) -> Option<tokio::sync::oneshot::Sender<ChargeLogSendMetadata>> {
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
        poll_fn(|ctx| {
            let mut tunn_sock = match self.0.try_lock() {
                Some(guard) => guard,
                None => {
                    log::error!("Failed to acquire lock on tunn_sock, will retry");
                    ctx.waker().wake_by_ref();
                    return Poll::Pending;
                }
            };

            if let Some(socket) = tunn_sock.get_tcp_socket() {
                socket.register_recv_waker(ctx.waker());

                if let tcp::State::CloseWait = socket.state() {
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
