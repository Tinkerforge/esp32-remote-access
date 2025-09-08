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

use rand_core::{OsRng, TryRngCore};
use smoltcp::{
    iface::{Config, SocketHandle, SocketSet},
    phy,
    socket::tcp::{ConnectError, ListenError, Socket},
    wire::{IpCidr, IpListenEndpoint},
};

use crate::{utils::now, wg_device::IsUp};

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
}

impl<'a, Device: phy::Device + IsUp> Interface<'a, Device> {
    pub fn new(device: Device, ip: IpCidr) -> Self {
        let mut config = Config::new(smoltcp::wire::HardwareAddress::Ip);
        let mut rng = [0u8; 8];
        OsRng.try_fill_bytes(&mut rng).unwrap();

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
        }
    }

    #[inline]
    pub fn get_socket(&self, handle: SocketHandle) -> &Socket<'_> {
        self.sockets.get::<Socket>(handle)
    }

    pub fn set_socket(&mut self, socket: Socket<'a>) -> SocketHandle {
        self.sockets.add(socket)
    }

    #[inline]
    pub fn poll(&mut self) {
        self.iface.poll(now(), &mut self.device, &mut self.sockets);
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
    }

    #[inline]
    pub fn send_slice(
        &mut self,
        handle: SocketHandle,
        slice: &[u8],
    ) -> Result<usize, smoltcp::socket::tcp::SendError> {
        let socket = self.sockets.get_mut::<Socket>(handle);
        let sent = socket.send_slice(slice)?;
        self.poll();
        Ok(sent)
    }

    #[inline]
    pub fn recv_slice(
        &mut self,
        handle: SocketHandle,
        slice: &mut [u8],
    ) -> Result<usize, smoltcp::socket::tcp::RecvError> {
        self.poll();
        let socket = self.sockets.get_mut::<Socket>(handle);
        let len = socket.recv_slice(slice)?;
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
        Ok(())
    }

    pub fn is_up(&self) -> bool {
        self.device.is_up()
    }
}
