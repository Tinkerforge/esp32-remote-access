use crate::{interface::Interface, wg_device::IsUp};
use smoltcp::{
    iface::SocketHandle,
    phy::{self},
    socket::tcp::{self, ConnectError, ListenError, RecvError},
    wire::IpListenEndpoint,
};
use std::{
    cell::RefCell,
    io::{Read, Write},
    rc::Rc,
};

/**
   This is an abstraction to be able to implement the std::io::Read and std::io::Write traits for
   smoltcp tcp sockets.
*/
#[derive(Clone)]
pub struct TcpStream<'a, Device>
where
    Device: phy::Device + Clone + IsUp,
{
    iface: Rc<RefCell<Interface<'a, Device>>>,
    handle: SocketHandle,
    buf: Vec<u8>,
}

impl<'a, Device: phy::Device + Clone + IsUp> TcpStream<'a, Device> {
    pub fn new(iface: Rc<RefCell<Interface<'a, Device>>>) -> Self {
        let rx_buf = tcp::SocketBuffer::new(vec![0; 65535]);

        //FIXME: Implement a buffer in the interface struct to handle bigger payloads.
        let tx_buf = tcp::SocketBuffer::new(vec![0; 5000000]);
        let tcp_socket = tcp::Socket::new(rx_buf, tx_buf);

        let handle = iface.borrow_mut().set_socket(tcp_socket);

        Self {
            iface,
            handle,
            buf: vec![],
        }
    }

    #[inline]
    pub fn poll(&mut self) {
        self.iface.borrow_mut().poll();
    }

    #[inline]
    pub fn is_open(&self) -> bool {
        let iface = self.iface.borrow_mut();
        let socket = iface.get_socket(self.handle);
        socket.is_open()
    }

    #[inline]
    pub fn can_recv(&self) -> bool {
        let iface = self.iface.borrow_mut();
        let socket = iface.get_socket(self.handle);
        socket.can_recv()
    }

    #[inline]
    fn may_recv(&self) -> bool {
        let iface = self.iface.borrow_mut();
        let socket = iface.get_socket(self.handle);
        socket.may_recv()
    }

    #[inline]
    pub fn can_send(&self) -> bool {
        let iface = self.iface.borrow_mut();
        let socket = iface.get_socket(self.handle);
        socket.can_send()
    }

    // for testing
    #[allow(dead_code)]
    #[inline]
    fn may_send(&self) -> bool {
        let iface = self.iface.borrow_mut();
        let socket = iface.get_socket(self.handle);
        socket.may_send()
    }

    // for testing
    #[allow(dead_code)]
    pub fn listen<T: Into<IpListenEndpoint>>(&mut self, endpoint: T) -> Result<(), ListenError> {
        self.iface.borrow_mut().listen(endpoint, self.handle)
    }

    #[inline]
    pub fn close(&mut self) {
        self.iface.borrow_mut().close(self.handle);
    }

    pub fn connect<T, U>(&mut self, remote: T, local: U) -> Result<(), ConnectError>
    where
        T: Into<smoltcp::wire::IpEndpoint>,
        U: Into<smoltcp::wire::IpListenEndpoint>,
    {
        self.iface.borrow_mut().connect(remote, local, self.handle)
    }

    pub fn is_up(&self) -> bool {
        self.iface.borrow().is_up()
    }
}

impl<Device: phy::Device + Clone + IsUp> Write for TcpStream<'_, Device> {
    #[inline]
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        Ok(self.buf.write(buf)?)
    }

    #[inline]
    fn flush(&mut self) -> std::io::Result<()> {
        match self
            .iface
            .borrow_mut()
            .send_slice(self.handle, &self.buf[..])
        {
            Ok(sent) => {
                if sent != self.buf.len() {
                    //FIXME: Implement a buffer in the interface struct to handle bigger payloads.
                    panic!("tx buffer is too small!");
                }
            }
            Err(e) => {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    format!("failed to send data: {:?}", e),
                ))
            }
        }
        self.buf.clear();
        Ok(())
    }
}

impl<Device: phy::Device + Clone + IsUp> Read for TcpStream<'_, Device> {
    #[inline]
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.iface.borrow_mut().poll();
        if !self.may_recv() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::BrokenPipe,
                "connection closed",
            ));
        } else if !self.can_recv() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::WouldBlock,
                "no data ready",
            ));
        }

        match self.iface.borrow_mut().recv_slice(self.handle, buf) {
            Ok(len) => Ok(len),
            Err(e) => match e {
                RecvError::Finished => Ok(0),
                RecvError::InvalidState => Err(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    "failed to recv data",
                )),
            },
        }
    }
}

#[cfg(test)]
pub mod test {
    use smoltcp::wire::{IpAddress, IpCidr};
    use std::io::{Read, Write};
    use std::{
        cell::RefCell,
        collections::VecDeque,
        rc::Rc,
        sync::{Arc, Mutex},
    };
    use wasm_bindgen_test::*;

    use crate::tests::LocalDevice;

    use super::TcpStream;

    #[wasm_bindgen_test]
    fn test_create_tcpstream() {
        let buf = Arc::new(Mutex::new(VecDeque::new()));
        let buf2 = Arc::new(Mutex::new(VecDeque::new()));
        let device = LocalDevice::new(buf, buf2);
        let ip = IpCidr::new(IpAddress::v4(192, 168, 69, 1), 24);
        let iface = crate::interface::Interface::new(device, ip);
        let iface = Rc::new(RefCell::new(iface));
        let _ = TcpStream::new(iface);
    }

    pub fn create_connected_stream_pair<'a>(
    ) -> (TcpStream<'a, LocalDevice>, TcpStream<'a, LocalDevice>) {
        let buf = Arc::new(Mutex::new(VecDeque::new()));
        let buf2 = Arc::new(Mutex::new(VecDeque::new()));
        let device = LocalDevice::new(buf.clone(), buf2.clone());
        let ip = IpCidr::new(IpAddress::v4(192, 168, 69, 1), 24);
        let iface = crate::interface::Interface::new(device, ip);
        let iface = Rc::new(RefCell::new(iface));
        let mut stream = TcpStream::new(iface);
        stream.listen(80).unwrap();

        let device2 = LocalDevice::new(buf2, buf);
        let ip2 = IpCidr::new(IpAddress::v4(192, 168, 69, 2), 24);
        let iface2 = crate::interface::Interface::new(device2, ip2);
        let iface2 = Rc::new(RefCell::new(iface2));
        let mut stream2 = TcpStream::new(iface2);

        let endpoint =
            smoltcp::wire::IpEndpoint::new(smoltcp::wire::IpAddress::v4(192, 168, 69, 1), 80);
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
