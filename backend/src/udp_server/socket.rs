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

use super::device::ManagementDevice;

pub struct ManagementSocket {
    charger_id: i32,
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
}

impl ManagementSocket {
    pub fn new(
        self_ip: Ipv4Addr,
        peer_ip: Ipv4Addr,
        remote_addr: SocketAddr,
        tunn: Tunn,
        rate_limiter: Arc<RateLimiter>,
        udp_socket: Arc<UdpSocket>,
        charger_id: i32,
    ) -> Self {
        let tunn = Arc::new(Mutex::new(tunn));

        let mut device = ManagementDevice::new(udp_socket.clone(), tunn.clone(), remote_addr);

        let mut config = Config::new(smoltcp::wire::HardwareAddress::Ip);
        config.random_seed = rand::random();
        let mut interface = Interface::new(config, &mut device, smoltcp::time::Instant::now());
        interface.update_ip_addrs(|ip_addrs| {
            log::debug!("listening on ip: {}", self_ip);
            let _ = ip_addrs.push(smoltcp::wire::IpCidr::new(
                smoltcp::wire::IpAddress::Ipv4(self_ip.into()),
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

        let management_sock = Self {
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
        };
        management_sock
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
            TunnResult::Err(err) => Err(format!("{:?}", err)),
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

    pub fn id(&self) -> i32 {
        self.charger_id
    }
}
