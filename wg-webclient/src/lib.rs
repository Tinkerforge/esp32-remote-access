mod utils;
pub mod wg_client;
mod stream;
mod handshake;
mod wg_device;
mod hyper_stream;
mod interval_handle;


use wasm_bindgen::prelude::*;

#[wasm_bindgen]
extern "C" {
    fn alert(s: &str);
}

#[macro_export]
macro_rules! console_log {
    ($($t:tt)*) => (crate::utils::log(&format_args!($($t)*).to_string()))
}

#[wasm_bindgen]
pub fn greet() {
    alert("Hello, wg-webclient!");
}

#[cfg(test)]
pub mod tests {
    use std::{ sync::{Arc, Mutex}, collections::VecDeque};

    use smoltcp::{iface::{Config, Interface, SocketSet}, wire::{IpAddress, IpCidr}, socket::tcp::{self, Socket}, phy};
    use wasm_bindgen_test::*;
    use crate::wg_device::*;
    use boringtun::x25519;

    wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_browser);

    pub struct LocalDevice {
        rx: Arc<Mutex<VecDeque<Vec<u8>>>>,
        tx: Arc<Mutex<VecDeque<Vec<u8>>>>,
    }

    impl LocalDevice {
        pub fn new(rx: Arc<Mutex<VecDeque<Vec<u8>>>>, tx: Arc<Mutex<VecDeque<Vec<u8>>>>) -> Self {
            Self {
                rx,
                tx,
            }
        }
    }

    impl phy::Device for LocalDevice {
        type RxToken<'a> = LocalRxToken;
        type TxToken<'a> = LocalTxToken;

        fn capabilities(&self) -> phy::DeviceCapabilities {
            let mut caps = phy::DeviceCapabilities::default();
            caps.medium = phy::Medium::Ip;
            caps.max_transmission_unit = 1500;
            caps
        }

        fn receive(&mut self, _: smoltcp::time::Instant) -> Option<(Self::RxToken<'_>, Self::TxToken<'_>)> {
            let mut rx = self.rx.lock().unwrap();
            if rx.is_empty() {
                return None
            }

            Some((
                LocalRxToken {
                    buf: rx.pop_front().unwrap(),
                },
                LocalTxToken {
                    queue: self.tx.clone(),
                },
            ))
        }

        fn transmit(&mut self, _: smoltcp::time::Instant) -> Option<Self::TxToken<'_>> {
            Some(LocalTxToken {
                queue: self.tx.clone(),
            })
        }
    }

    pub struct LocalRxToken {
        buf: Vec<u8>,
    }

    impl phy::RxToken for LocalRxToken {
        fn consume<R, F>(mut self, f: F) -> R
            where
            F: FnOnce(&mut [u8]) -> R,
        {
            f(&mut self.buf[..])
        }
    }

    pub struct LocalTxToken {
        queue: Arc<Mutex<VecDeque<Vec<u8>>>>,
    }

    impl phy::TxToken for LocalTxToken {
        fn consume<R, F>(self, size: usize, f: F) -> R
            where
            F: FnOnce(&mut [u8]) -> R,
        {
            let mut buf = vec![0u8; size];
            let res = f(&mut buf[..]);
            let mut queue = self.queue.lock().unwrap();
            queue.push_back(buf);
            res
        }
    }

    fn create_WgTunDevice() -> WgTunDevice {
        WgTunDevice::new(
            x25519::StaticSecret::random_from_rng(rand_core::OsRng),
            x25519::PublicKey::from(&x25519::StaticSecret::random_from_rng(rand_core::OsRng))
        ).unwrap()
    }

    #[wasm_bindgen_test]
    fn test_create_WgTunDevice() {
        let _ = create_WgTunDevice();
    }

    struct SocketWrap<'a> {
        pub device: LocalDevice,
        pub socket: Socket<'a>,
        pub iface: Interface,
    }

    // fn create_tcp_socket<'a>(ip: (u8, u8, u8, u8)) -> SocketWrap<'a> {
    //     let mut device = create_WgTunDevice();
    //     let mut config = Config::new(smoltcp::wire::HardwareAddress::Ip);
    //     let mut rng = [0u8; 8];
    //     getrandom::getrandom(&mut rng).unwrap();
    //     config.random_seed = u64::from_ne_bytes(rng);

    //     let now = web_sys::window()
    //         .expect("not in a browser")
    //         .performance()
    //         .expect("performance object not available")
    //         .now();
    //     let now = smoltcp::time::Instant::from_millis(now as i64);

    //     let mut iface = Interface::new(config, &mut device, now);
    //     iface.update_ip_addrs(|ip_addrs| {
    //         ip_addrs.push(IpCidr::new(IpAddress::v4(ip.0, ip.1, ip.2, ip.3), 24)).unwrap();
    //     });
    //     iface.routes_mut().add_default_ipv4_route(Ipv4Address::new(192, 168, 69, 1)).unwrap();

    //     let tcp_rx_buf = tcp::SocketBuffer::new(vec![0; 1500]);
    //     let tcp_tx_buf = tcp::SocketBuffer::new(vec![0; 1500]);
    //     let tcp_socket = tcp::Socket::new(tcp_rx_buf, tcp_tx_buf);
    //     let mut sockets = SocketSet::new(vec![]);
    //     let tcp_handle = sockets.add(tcp_socket);

    //     SocketWrap {
    //         sockets,
    //         tcp_handle,
    //         iface,
    //     }
    // }

    // fn create_two_tcp_sockets<'a>() -> (SocketWrap<'a>, SocketWrap<'a>) {
    //     (
    //         create_tcp_socket((192, 168, 69, 1)),
    //         create_tcp_socket((192, 168, 69, 2)),
    //     )
    // }

    fn create_local_socket<'a>(device_rx: Arc<Mutex<VecDeque<Vec<u8>>>>, device_tx: Arc<Mutex<VecDeque<Vec<u8>>>>, ip: (u8, u8, u8, u8)) -> SocketWrap<'a> {
        let mut device = LocalDevice::new(device_rx, device_tx);
        let mut config = Config::new(smoltcp::wire::HardwareAddress::Ip);
        let mut rng = [0u8; 8];
        getrandom::getrandom(&mut rng).unwrap();
        config.random_seed = u64::from_ne_bytes(rng);

        let now = web_sys::window()
            .expect("not in a browser")
            .performance()
            .expect("performance object not available")
            .now();
        let now = smoltcp::time::Instant::from_millis(now as i64);

        let mut iface = Interface::new(config, &mut device, now);
        iface.update_ip_addrs(|ip_addrs| {
            ip_addrs.push(IpCidr::new(IpAddress::v4(ip.0, ip.1, ip.2, ip.3), 24)).unwrap();
        });

        let tcp_rx_buf = tcp::SocketBuffer::new(vec![0; 1500]);
        let tcp_tx_buf = tcp::SocketBuffer::new(vec![0; 1500]);
        let socket = tcp::Socket::new(tcp_rx_buf, tcp_tx_buf);

        SocketWrap {
            device,
            socket,
            iface,
        }
    }

    fn create_two_local_sockets<'a>(buf1: Arc<Mutex<VecDeque<Vec<u8>>>>, buf2: Arc<Mutex<VecDeque<Vec<u8>>>>) -> (SocketWrap<'a>, SocketWrap<'a>) {
        (
            create_local_socket(buf1.clone(), buf2.clone(), (192, 168, 69, 1)),
            create_local_socket(buf2.clone(), buf1.clone(), (192, 168, 69, 2)),
        )
    }

    #[wasm_bindgen_test]
    fn test_create_local_socket() {
        let rx = Arc::new(Mutex::new(VecDeque::new()));
        let tx = Arc::new(Mutex::new(VecDeque::new()));
        let _ = create_local_socket(rx, tx, (192, 168, 69, 1));
    }

    #[wasm_bindgen_test]
    fn test_connecting_local_sockets() {
        let buf1 = Arc::new(Mutex::new(VecDeque::new()));
        let buf2 = Arc::new(Mutex::new(VecDeque::new()));
        let (mut socket1, mut socket2) = create_two_local_sockets(buf1, buf2);

        let mut sock_set1 = SocketSet::new(vec![]);
        let sock1_handle = sock_set1.add(socket1.socket);
        let sock1 = sock_set1.get_mut::<tcp::Socket>(sock1_handle);
        sock1.listen(80).unwrap();

        let mut sock_set2 = SocketSet::new(vec![]);
        let sock2_handle = sock_set2.add(socket2.socket);
        {
            let sock2 = sock_set2.get_mut::<tcp::Socket>(sock2_handle);
            let endpoint = smoltcp::wire::IpEndpoint {
                addr: IpAddress::v4(192, 168, 69, 1),
                port: 80,
            };
            sock2.connect(socket2.iface.context(), endpoint, 80).unwrap();
        }

        let mut send = false;
        loop {
            let now = web_sys::window()
                .expect("not in a browser")
                .performance()
                .expect("performance object not available")
                .now();
            let now = smoltcp::time::Instant::from_millis(now as i64);
            let test = socket2.iface.poll(now, &mut socket2.device, &mut sock_set2);
            if !send {
                assert_eq!(test, true);
            }
            socket1.iface.poll(now, &mut socket1.device, &mut sock_set1);

            let sock2 = sock_set2.get_mut::<tcp::Socket>(sock2_handle);
            if sock2.can_send() && !send {
                sock2.send_slice(b"hello world").unwrap();
                send = true;
            }

            let sock1 = sock_set1.get_mut::<tcp::Socket>(sock1_handle);
            if sock1.can_recv() {
                let mut buf = [0u8; 1500];
                let len = sock1.recv_slice(&mut buf).unwrap();
                assert_eq!(&buf[..len], b"hello world");
                break;
            }
        }
    }


}
