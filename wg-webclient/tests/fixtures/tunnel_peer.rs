//! Encrypted WireGuard/smoltcp peer for the browser integration tests.
//! Defaults to port 18081; Playwright passes 0 to use an ephemeral port.
use std::{
    collections::VecDeque,
    error::Error,
    io::ErrorKind,
    net::{TcpListener, TcpStream},
    thread,
    time::{Duration, Instant},
};

use boringtun::{
    noise::{Tunn, TunnResult},
    x25519,
};
use rand_core::{OsRng, TryRngCore};
use smoltcp::{
    iface::{Config, Interface, SocketHandle, SocketSet},
    phy::{self, ChecksumCapabilities, DeviceCapabilities, Medium},
    socket::tcp,
    time::Instant as NetworkInstant,
    wire::{
        HardwareAddress, Icmpv6Packet, Icmpv6Repr, IpAddress, IpCidr, IpProtocol, Ipv6Packet,
        Ipv6Repr,
    },
};
use tungstenite::{
    accept_hdr,
    handshake::{
        derive_accept_key,
        server::{Request, Response},
    },
    Message, WebSocket,
};

type Result<T> = std::result::Result<T, Box<dyn Error + Send + Sync>>;

#[derive(Default)]
struct Device {
    rx: VecDeque<Vec<u8>>,
    tx: VecDeque<Vec<u8>>,
}

struct Rx(Vec<u8>);
struct Tx<'a>(&'a mut VecDeque<Vec<u8>>);

impl phy::RxToken for Rx {
    fn consume<R, F: FnOnce(&[u8]) -> R>(self, f: F) -> R {
        f(&self.0)
    }
}

impl phy::TxToken for Tx<'_> {
    fn consume<R, F: FnOnce(&mut [u8]) -> R>(self, size: usize, f: F) -> R {
        let mut packet = vec![0; size];
        let result = f(&mut packet);
        self.0.push_back(packet);
        result
    }
}

impl phy::Device for Device {
    type RxToken<'a> = Rx;
    type TxToken<'a> = Tx<'a>;

    fn receive(&mut self, _: NetworkInstant) -> Option<(Rx, Tx<'_>)> {
        Some((Rx(self.rx.pop_front()?), Tx(&mut self.tx)))
    }

    fn transmit(&mut self, _: NetworkInstant) -> Option<Tx<'_>> {
        Some(Tx(&mut self.tx))
    }

    fn capabilities(&self) -> DeviceCapabilities {
        let mut capabilities = DeviceCapabilities::default();
        capabilities.medium = Medium::Ip;
        capabilities.max_transmission_unit = 1420;
        capabilities
    }
}

fn discovery_reply(data: &[u8], local: IpAddress) -> Option<Vec<u8>> {
    let IpAddress::Ipv6(local) = local else {
        return None;
    };
    let request = Ipv6Packet::new_checked(data).ok()?;
    if request.next_header() != IpProtocol::Icmpv6
        || request.dst_addr() != "ff02::1".parse::<std::net::Ipv6Addr>().ok()?
    {
        return None;
    }
    let packet = Icmpv6Packet::new_checked(request.payload()).ok()?;
    let checksums = ChecksumCapabilities::default();
    let Icmpv6Repr::EchoRequest {
        ident,
        seq_no,
        data,
    } = Icmpv6Repr::parse(
        &request.src_addr(),
        &request.dst_addr(),
        &packet,
        &checksums,
    )
    .ok()?
    else {
        return None;
    };
    let echo = Icmpv6Repr::EchoReply {
        ident,
        seq_no,
        data,
    };
    let header = Ipv6Repr {
        src_addr: local,
        dst_addr: request.src_addr(),
        next_header: IpProtocol::Icmpv6,
        payload_len: echo.buffer_len(),
        hop_limit: 64,
    };
    let mut reply = vec![0; 40 + echo.buffer_len()];
    let mut packet = Ipv6Packet::new_unchecked(&mut reply);
    header.emit(&mut packet);
    echo.emit(
        &local,
        &request.src_addr(),
        &mut Icmpv6Packet::new_unchecked(packet.payload_mut()),
        &checksums,
    );
    Some(reply)
}

fn send_network(socket: &mut WebSocket<TcpStream>, data: &[u8]) -> Result<()> {
    socket.send(Message::Binary(data.to_vec().into()))?;
    Ok(())
}

fn serve(stream: TcpStream) -> Result<()> {
    let mut profile = String::new();
    // tungstenite requires an unboxed HTTP response as the callback's error.
    #[allow(clippy::result_large_err)]
    let mut socket = accept_hdr(stream, |request: &Request, response: Response| {
        profile = request.uri().path().to_owned();
        Ok(response)
    })
    .map_err(|error| format!("WebSocket handshake: {error}"))?;
    socket.get_mut().set_nonblocking(true)?;
    let ipv6 = profile != "/v4";
    let local: IpAddress = if profile == "/v6-discovery" {
        let mut address = [0; 16];
        OsRng.try_fill_bytes(&mut address)?;
        address[0] = 0xfd;
        std::net::Ipv6Addr::from(address).into()
    } else if ipv6 {
        "fd00::2".parse().unwrap()
    } else {
        "10.123.1.2".parse().unwrap()
    };
    let secret = x25519::StaticSecret::from([2; 32]);
    let peer = x25519::PublicKey::from(&x25519::StaticSecret::from([1; 32]));
    let mut tunnel = Tunn::new(secret, peer, Some([3; 32]), None, 2, None);
    let started = Instant::now();
    let mut device = Device::default();
    let mut config = Config::new(HardwareAddress::Ip);
    config.random_seed = OsRng.try_next_u64()?;
    let mut iface = Interface::new(config, &mut device, NetworkInstant::from_millis(0));
    // There is one authenticated peer, so every destination is on-link.
    iface.update_ip_addrs(|addresses| addresses.push(IpCidr::new(local, 0)).unwrap());
    let mut sockets = SocketSet::new(vec![]);
    struct Connection {
        handle: SocketHandle,
        request: Vec<u8>,
        answered: bool,
    }
    let mut connections = Vec::new();
    for _ in 0..8 {
        let buffer = || tcp::SocketBuffer::new(vec![0; 16384]);
        let mut socket = tcp::Socket::new(buffer(), buffer());
        socket.listen(80)?;
        connections.push(Connection {
            handle: sockets.add(socket),
            request: Vec::new(),
            answered: false,
        });
    }
    let mut output = vec![0; u16::MAX as usize + 32];
    loop {
        match socket.read() {
            Ok(Message::Binary(data)) => {
                let mut input = data.as_ref();
                loop {
                    match tunnel.decapsulate(None, input, &mut output) {
                        TunnResult::WriteToNetwork(packet) => {
                            send_network(&mut socket, packet)?;
                            input = &[];
                        }
                        TunnResult::WriteToTunnelV4(packet, _)
                        | TunnResult::WriteToTunnelV6(packet, _) => {
                            if packet[0] >> 4 == if ipv6 { 6 } else { 4 } {
                                if let Some(reply) = discovery_reply(packet, local) {
                                    device.tx.push_back(reply);
                                } else {
                                    device.rx.push_back(packet.to_vec());
                                }
                            }
                            break;
                        }
                        TunnResult::Done | TunnResult::Err(_) => break,
                    }
                }
            }
            Ok(Message::Close(_))
            | Err(tungstenite::Error::ConnectionClosed | tungstenite::Error::AlreadyClosed) => {
                return Ok(())
            }
            Err(tungstenite::Error::Io(error)) if error.kind() == ErrorKind::WouldBlock => {}
            Err(error) => return Err(error.into()),
            _ => {}
        }
        let now = NetworkInstant::from_millis(started.elapsed().as_millis() as i64);
        iface.poll(now, &mut device, &mut sockets);
        for connection in &mut connections {
            let stream = sockets.get_mut::<tcp::Socket>(connection.handle);
            if stream.can_recv() {
                stream.recv(|data| {
                    connection.request.extend_from_slice(data);
                    (data.len(), ())
                })?;
            }
            if !connection.answered
                && stream.can_send()
                && connection
                    .request
                    .windows(4)
                    .any(|bytes| bytes == b"\r\n\r\n")
            {
                let request = String::from_utf8_lossy(&connection.request);
                let websocket = request.starts_with("GET /ws ");
                let response = if websocket {
                    let key = request
                        .lines()
                        .find_map(|line| {
                            let (name, value) = line.split_once(':')?;
                            name.eq_ignore_ascii_case("Sec-WebSocket-Key")
                                .then_some(value.trim())
                        })
                        .ok_or("missing inner WebSocket key")?;
                    let mut response = format!("HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: {}\r\n\r\n", derive_accept_key(key.as_bytes())).into_bytes();
                    response.extend_from_slice(b"\x81\x07peer-ws");
                    response
                } else {
                    let body = if ipv6 { "peer-v6" } else { "peer-v4" };
                    format!(
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    )
                    .into_bytes()
                };
                let sent = stream.send_slice(&response)?;
                if sent != response.len() {
                    return Err("incomplete test response".into());
                }
                connection.answered = true;
                if !websocket {
                    stream.close();
                }
            }
        }
        iface.poll(now, &mut device, &mut sockets);
        while let Some(packet) = device.tx.pop_front() {
            if let TunnResult::WriteToNetwork(data) = tunnel.encapsulate(&packet, &mut output) {
                send_network(&mut socket, data)?;
            }
        }
        if let TunnResult::WriteToNetwork(data) = tunnel.update_timers(&mut output) {
            send_network(&mut socket, data)?;
        }
        match socket.flush() {
            Ok(()) => {}
            Err(tungstenite::Error::Io(error)) if error.kind() == ErrorKind::WouldBlock => {}
            Err(error) => return Err(error.into()),
        }
        thread::sleep(Duration::from_millis(2));
    }
}

fn main() -> Result<()> {
    let port = std::env::args()
        .nth(1)
        .map(|value| value.parse())
        .transpose()?
        .unwrap_or(18081);
    let ipv6 = TcpListener::bind(("::1", port))?;
    let port = ipv6.local_addr()?.port();
    let ipv4 = TcpListener::bind(("127.0.0.1", port))?;
    println!("LISTENING {port}");
    let accept = |listener: TcpListener| {
        for stream in listener.incoming() {
            let Ok(stream) = stream else { break };
            thread::spawn(move || {
                if let Err(error) = serve(stream) {
                    eprintln!("Encrypted test peer: {error}");
                }
            });
        }
    };
    thread::spawn(move || accept(ipv4));
    accept(ipv6);
    Ok(())
}
