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

use std::rc::Weak;

use getrandom::SysRng;
use rand_core::TryRng;
use smoltcp::{
    iface::{Config, SocketHandle, SocketSet},
    phy,
    socket::{icmp, tcp::{ConnectError, ListenError, Socket}},
    time::Instant,
    wire::{IpAddress, IpCidr, IpListenEndpoint},
};

use crate::{poll_timer::PollTimer, utils::now, wg_device::IsUp};

/**
    Creates an abstraction layer between smoltcp and this crates TcpStream to be able to have
    multiple TcpStreams at once.
*/
pub struct Interface<'a, Device>
where
    Device: phy::Device + IsUp,
{
    device: Device,
    sockets: SocketSet<'a>,
    iface: smoltcp::iface::Interface,
    poll_timer: Weak<PollTimer>,
}

impl<'a, Device: phy::Device + IsUp> Interface<'a, Device> {
    pub fn new(device: Device, ip: IpCidr) -> Self {
        let mut config = Config::new(smoltcp::wire::HardwareAddress::Ip);
        let mut rng = [0u8; 8];
        SysRng.try_fill_bytes(&mut rng).unwrap();

        config.random_seed = u64::from_ne_bytes(rng);

        let mut device = device;
        let mut iface = smoltcp::iface::Interface::new(config, &mut device, now());
        iface.update_ip_addrs(|addrs| {
            // will not panic because vec len is always just 1
            addrs.push(ip).unwrap();
        });
        let socket_set = SocketSet::new(vec![]);

        Self {
            device,
            sockets: socket_set,
            iface,
            poll_timer: Weak::new(),
        }
    }

    #[inline]
    pub fn get_socket(&self, handle: SocketHandle) -> &Socket<'_> {
        self.sockets.get::<Socket>(handle)
    }

    pub fn set_socket(&mut self, socket: Socket<'a>) -> SocketHandle {
        self.sockets.add(socket)
    }

    /// Add a new ICMP socket to the interface and return its handle.
    /// Used for ICMPv6 peer discovery when the wg-webclient connects via IPv6.
    pub fn set_icmp_socket(
        &mut self,
        rx_buffer: icmp::PacketBuffer<'a>,
        tx_buffer: icmp::PacketBuffer<'a>,
    ) -> SocketHandle {
        let socket = icmp::Socket::new(rx_buffer, tx_buffer);
        self.sockets.add(socket)
    }

    /// Get a mutable reference to the ICMP socket with the given handle.
    pub fn get_icmp_socket_mut(&mut self, handle: SocketHandle) -> &mut icmp::Socket<'a> {
        self.sockets.get_mut::<icmp::Socket>(handle)
    }

    pub fn add_ip_address(&mut self, address: IpCidr) {
        self.iface.update_ip_addrs(|addresses| addresses.push(address).unwrap());
    }

    pub fn select_address_family(&mut self, peer: IpAddress) {
        self.iface.update_ip_addrs(|addresses| addresses.retain(|address|
            matches!((address.address(), peer),
                (IpAddress::Ipv4(_), IpAddress::Ipv4(_)) |
                (IpAddress::Ipv6(_), IpAddress::Ipv6(_)))));
    }

    pub fn remove_socket(&mut self, handle: SocketHandle) {
        self.sockets.remove(handle);
    }

    #[inline]
    pub fn poll(&mut self) {
        self.iface.poll(now(), &mut self.device, &mut self.sockets);
    }

    pub fn poll_at(&mut self) -> Option<Instant> {
        self.iface.poll_at(now(), &self.sockets)
    }

    pub fn set_poll_timer(&mut self, timer: Weak<PollTimer>) {
        self.poll_timer = timer;
    }

    pub fn poll_before_read(&mut self) {
        // Standalone streams can drive the interface without a worker timer.
        if self.poll_timer.strong_count() == 0 {
            self.poll();
        }
    }

    // for testing
    #[allow(dead_code)]
    pub fn listen<T: Into<IpListenEndpoint>>(
        &mut self,
        endpoint: T,
        handle: SocketHandle,
    ) -> Result<(), ListenError> {
        let socket = self.sockets.get_mut::<Socket>(handle);
        socket.listen(endpoint.into())?;
        Ok(())
    }

    #[inline]
    pub fn close(&mut self, handle: SocketHandle) {
        let socket = self.sockets.get_mut::<Socket>(handle);
        socket.close();
        if let Some(timer) = self.poll_timer.upgrade() {
            timer.request();
        }
    }

    #[inline]
    pub fn send_slice(
        &mut self,
        handle: SocketHandle,
        slice: &[u8],
    ) -> Result<usize, smoltcp::socket::tcp::SendError> {
        let socket = self.sockets.get_mut::<Socket>(handle);
        let sent = socket.send_slice(slice)?;
        if let Some(timer) = self.poll_timer.upgrade() {
            if sent != 0 {
                timer.request();
            }
        } else {
            self.poll();
        }
        Ok(sent)
    }

    #[inline]
    pub fn recv_slice(
        &mut self,
        handle: SocketHandle,
        slice: &mut [u8],
    ) -> Result<usize, smoltcp::socket::tcp::RecvError> {
        let socket = self.sockets.get_mut::<Socket>(handle);
        let len = socket.recv_slice(slice)?;
        if len != 0 {
            // Reading can reopen the receive window and require a TCP update.
            if let Some(timer) = self.poll_timer.upgrade() {
                timer.request();
            }
        }
        Ok(len)
    }

    pub fn connect<T, U>(
        &mut self,
        remote: T,
        local: U,
        handle: SocketHandle,
    ) -> Result<(), ConnectError>
    where
        T: Into<smoltcp::wire::IpEndpoint>,
        U: Into<smoltcp::wire::IpListenEndpoint>,
    {
        let socket = self.sockets.get_mut::<Socket>(handle);
        socket.connect(self.iface.context(), remote.into(), local.into())?;
        if let Some(timer) = self.poll_timer.upgrade() {
            timer.request();
        }
        Ok(())
    }

    pub fn device(&self) -> &Device {
        &self.device
    }

    pub fn is_up(&self) -> bool {
        self.device.is_up()
    }
}
