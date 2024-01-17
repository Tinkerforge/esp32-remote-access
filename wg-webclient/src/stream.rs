use std::io::{Read, Write};
use smoltcp::{iface::{SocketSet, Interface, SocketHandle, Config}, phy::{self}, wire::{IpCidr, IpListenEndpoint}, socket::tcp::{self, ConnectError, RecvError, ListenError}};
use crate::utils::now;

pub struct TcpStream<'a, Device>
where Device: phy::Device {
    socket_set: SocketSet<'a>,
    iface: Interface,
    handle: SocketHandle,
    device: Device,
    buf: Vec<u8>,
}

impl<'a, Device: phy::Device> TcpStream <'a, Device> {
    pub fn new(device: Device, ip: IpCidr) -> Self {
        let mut config = Config::new(smoltcp::wire::HardwareAddress::Ip);
        let mut rng = [0u8; 8];
        getrandom::getrandom(&mut rng).unwrap();

        config.random_seed = u64::from_ne_bytes(rng);

        let mut device = device;
        let mut iface = Interface::new(config, &mut device, now());
        iface.update_ip_addrs(|addrs| {
            // will not panic because vec len is always just 1
            addrs.push(ip).unwrap();
        });

        let rx_buf = tcp::SocketBuffer::new(vec![0; 500000]);
        let tx_buf = tcp::SocketBuffer::new(vec![0; 2048]);
        let tcp_socket = tcp::Socket::new(rx_buf, tx_buf);

        let mut socket_set = SocketSet::new(vec![]);
        let handle = socket_set.add(tcp_socket);

        Self {
            socket_set,
            iface,
            handle,
            device,
            buf: vec![],
        }
    }

    #[inline]
    pub fn is_open(&self) -> bool {
        let socket = self.socket_set.get::<tcp::Socket>(self.handle);
        socket.is_open()
    }

    #[inline]
    pub fn can_recv(&mut self) -> bool {
        let socket = self.socket_set.get::<tcp::Socket>(self.handle);
        socket.can_recv()
    }

    #[inline]
    pub fn can_send(&mut self) -> bool {
        let socket = self.socket_set.get::<tcp::Socket>(self.handle);
        socket.can_send()
    }

    // for testing
    #[allow(dead_code)]
    #[inline]
    fn may_recv(&mut self) -> bool {
        let socket = self.socket_set.get::<tcp::Socket>(self.handle);
        socket.may_recv()
    }

    // for testing
    #[allow(dead_code)]
    #[inline]
    fn may_send(&mut self) -> bool {
        let socket = self.socket_set.get::<tcp::Socket>(self.handle);
        socket.may_send()
    }

    #[inline]
    pub fn poll(&mut self) {
        self.iface.poll(now(), &mut self.device, &mut self.socket_set);
    }

    // for testing
    #[allow(dead_code)]
    pub fn listen<T: Into<IpListenEndpoint>>(&mut self, endpoint: T) -> Result<(), ListenError> {
        let socket = self.socket_set.get_mut::<tcp::Socket>(self.handle);
        socket.listen(endpoint.into())?;
        Ok(())
    }

    pub fn close(&mut self) {
        let socket = self.socket_set.get_mut::<tcp::Socket>(self.handle);
        socket.close();
    }

    pub fn connect<T, U>(&mut self, remote: T, local: U) -> Result<(), ConnectError>
    where T: Into<smoltcp::wire::IpEndpoint>,
          U: Into<smoltcp::wire::IpListenEndpoint> {
        let socket = self.socket_set.get_mut::<tcp::Socket>(self.handle);
        socket.connect(self.iface.context(), remote.into(), local.into())?;
        Ok(())
    }
}

impl<Device: phy::Device> Write for TcpStream<'_, Device> {

    #[inline]
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        Ok(self.buf.write(buf)?)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        let socket = self.socket_set.get_mut::<tcp::Socket>(self.handle);
        match socket.send_slice(&self.buf[..]) {
            Ok(_) =>(),
            Err(e) => return Err(std::io::Error::new(std::io::ErrorKind::Other, format!("failed to send data: {:?}", e))),
        }

        self.iface.poll(now(), &mut self.device, &mut self.socket_set);
        self.buf.clear();
        Ok(())
    }
}

impl<Device: phy::Device> Read for TcpStream<'_, Device> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.iface.poll(now(), &mut self.device, &mut self.socket_set);
        if !self.can_recv() {
            return Err(std::io::Error::new(std::io::ErrorKind::WouldBlock, "no data ready"))
        }

        let socket = self.socket_set.get_mut::<tcp::Socket>(self.handle);
        match socket.recv_slice(buf) {
            Ok(len) => Ok(len),
            Err(e) => match e {
                RecvError::Finished => Ok(0),
                RecvError::InvalidState => Err(std::io::Error::new(std::io::ErrorKind::Other, "failed to recv data"))
            },
        }
    }
}

#[cfg(test)]
pub mod test {
    use std::{sync::{Arc, Mutex}, collections::VecDeque};
    use std::io::{Read, Write};
    use smoltcp::wire::{IpAddress, IpCidr};
    use wasm_bindgen_test::*;

    use crate::tests::LocalDevice;

    use super::TcpStream;

    #[wasm_bindgen_test]
    fn test_create_tcpstream() {
        let buf = Arc::new(Mutex::new(VecDeque::new()));
        let buf2 = Arc::new(Mutex::new(VecDeque::new()));
        let device = LocalDevice::new(buf, buf2);
        let ip = IpCidr::new(IpAddress::v4(192, 168, 69, 1), 24);
        let _ =TcpStream::new(device, ip);
    }

    pub fn create_connected_stream_pair<'a>() -> (TcpStream<'a, LocalDevice>, TcpStream<'a, LocalDevice>) {
        let buf = Arc::new(Mutex::new(VecDeque::new()));
        let buf2 = Arc::new(Mutex::new(VecDeque::new()));
        let device = LocalDevice::new(buf.clone(), buf2.clone());
        let ip = IpCidr::new(IpAddress::v4(192, 168, 69, 1), 24);
        let mut stream = TcpStream::new(device, ip);
        stream.listen(80).unwrap();

        let device2 = LocalDevice::new(buf2, buf);
        let ip2 = IpCidr::new(IpAddress::v4(192, 168, 69, 2), 24);
        let mut stream2 = TcpStream::new(device2, ip2);

        let endpoint = smoltcp::wire::IpEndpoint::new(smoltcp::wire::IpAddress::v4(192, 168, 69, 1), 80);
        stream2.connect(endpoint, 80).unwrap();

        loop {
            stream.poll();
            stream2.poll();
            if stream.may_send() && stream2.may_recv() {
                break;
            }
        }

        (stream, stream2)
    }

    #[wasm_bindgen_test]
    fn test_stream_connect() {
        let (mut stream, mut stream2) = create_connected_stream_pair();
        let len = stream.write(b"hello world").unwrap();
        assert_eq!(len, 11);
        stream.flush().unwrap();
        let mut buf = [0u8; 1500];

        let len = stream2.read(&mut buf).unwrap();
        assert_eq!(len, 11);
        assert_eq!(&buf[..len], b"hello world");

        // test both ways
        let len = stream2.write(b"hello world").unwrap();
        assert_eq!(len, 11);
        stream2.flush().unwrap();

        let len = stream.read(&mut buf).unwrap();
        assert_eq!(len, 11);
        assert_eq!(&buf[..len], b"hello world");
    }
}
