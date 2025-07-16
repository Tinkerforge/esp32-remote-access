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
    net::{Ipv4Addr, SocketAddr, UdpSocket},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use boringtun::noise::{rate_limiter::RateLimiter, Tunn, TunnResult};
use smoltcp::{
    iface::{self, Config, Interface, SocketHandle, SocketSet},
    socket::udp,
};

use super::{device::ManagementDevice, packet::ManagementPacket};

pub struct ManagementSocket {
    charger_id: uuid::Uuid,
    sock_handle: SocketHandle,
    sockets: SocketSet<'static>,
    iface: iface::Interface,
    device: ManagementDevice,
    tunn: Arc<Mutex<Tunn>>,
    rate_limiter: Arc<RateLimiter>,
    peer_ip: Ipv4Addr,
    remote_addr: SocketAddr,
    udp_socket: Arc<UdpSocket>,
    last_seen: Instant,
    out_sequence: u16,
}

impl std::fmt::Debug for ManagementSocket {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ManagementSocket")
            .field("charger_id", &self.charger_id)
            .finish()
    }
}

impl ManagementSocket {
    pub fn new(
        self_ip: Ipv4Addr,
        peer_ip: Ipv4Addr,
        remote_addr: SocketAddr,
        tunn: Tunn,
        rate_limiter: Arc<RateLimiter>,
        udp_socket: Arc<UdpSocket>,
        charger_id: uuid::Uuid,
    ) -> Self {
        let tunn = Arc::new(Mutex::new(tunn));

        let mut device = ManagementDevice::new(udp_socket.clone(), tunn.clone(), remote_addr);

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
            tunn,
            rate_limiter,
            peer_ip,
            remote_addr,
            udp_socket,
            last_seen: Instant::now(),
            out_sequence: 1,
        }
    }

    pub fn reset(&mut self) {
        let mut tunn = self.tunn.lock().unwrap();
        tunn.clear_sessions();
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
        match self.udp_socket.send_to(data, self.remote_addr) {
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
        let mut tunn = self.tunn.lock().unwrap();
        let mut dst = vec![0; data.len()];
        match tunn.decapsulate(None, data, &mut dst) {
            TunnResult::WriteToNetwork(data) => {
                self.send_slice(data)?;
                while let TunnResult::WriteToNetwork(data) =
                    tunn.decapsulate(None, &Vec::new(), &mut dst)
                {
                    self.send_slice(data)?;
                }
                self.last_seen = Instant::now();
                Ok(Vec::new())
            }
            TunnResult::WriteToTunnelV4(data, _) => {
                drop(tunn);
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
}
