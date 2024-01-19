use smoltcp::{iface::{SocketSet, Config, SocketHandle}, phy, wire::{IpCidr, IpListenEndpoint}, socket::tcp::{Socket, ListenError, ConnectError}};

use crate::utils::now;



pub struct Interface<'a, Device>
where Device: phy::Device {
    device: Device,
    sockets: SocketSet<'a>,
    iface: smoltcp::iface::Interface,
}

impl<'a, Device: phy::Device> Interface<'a, Device> {
    pub fn new(device: Device, ip: IpCidr) -> Self {
        let mut config = Config::new(smoltcp::wire::HardwareAddress::Ip);
        let mut rng = [0u8; 8];
        getrandom::getrandom(&mut rng).unwrap();

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
    pub fn get_socket(&self, handle: SocketHandle) -> &Socket {
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
    pub fn listen<T: Into<IpListenEndpoint>>(&mut self, endpoint: T, handle: SocketHandle) -> Result<(), ListenError> {
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
    pub fn send_slice(&mut self, handle: SocketHandle, slice: &[u8]) -> Result<(), smoltcp::socket::tcp::SendError> {
        let socket = self.sockets.get_mut::<Socket>(handle);
        socket.send_slice(slice)?;
        self.poll();
        Ok(())
    }

    #[inline]
    pub fn recv_slice(&mut self, handle: SocketHandle, slice: &mut [u8]) -> Result<usize, smoltcp::socket::tcp::RecvError> {
        self.poll();
        let socket = self.sockets.get_mut::<Socket>(handle);
        let len = socket.recv_slice(slice)?;
        Ok(len)
    }

    pub fn connect<T, U>(&mut self, remote: T, local: U, handle: SocketHandle) -> Result<(), ConnectError>
    where T: Into<smoltcp::wire::IpEndpoint>,
          U: Into<smoltcp::wire::IpListenEndpoint> {
        let socket = self.sockets.get_mut::<Socket>(handle);
        socket.connect(self.iface.context(), remote.into(), local.into())?;
        Ok(())
    }
}
