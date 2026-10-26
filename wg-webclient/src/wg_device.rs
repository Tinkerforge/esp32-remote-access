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

use boringtun::noise::errors::WireGuardError;
use boringtun::noise::rate_limiter::RateLimiter;
use boringtun::{
    noise::{Tunn, TunnResult},
    x25519,
};
use pcap_file::pcapng::blocks::interface_description::InterfaceDescriptionBlock;
use pcap_file::pcapng::PcapNgWriter;
use rand_core::{OsRng, TryRngCore};
use smoltcp::phy::{self, DeviceCapabilities, Medium};
use smoltcp::time::Instant;
use wasm_bindgen_futures::spawn_local;
use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::{Rc, Weak};
use std::sync::Arc;

use crate::pcap_logging_enabled;
use crate::interval_handle::IntervalHandle;
use web_sys::wasm_bindgen::closure::Closure;
use web_sys::wasm_bindgen::{JsCast, JsValue};
use web_sys::{MessageEvent, WebSocket};

#[derive(PartialEq, Clone, Copy)]
pub enum WsConnectionState {
    Connected,
    Disconnected,
}


/// This struct emulates the hardware layer for smoltcp.
/// This is done by encoding outgoing packets before sending them over Websocket
/// and decoding incoming packets from Websocket and storing them in a queue.
#[derive(Clone)]
pub struct WgTunDevice {
    pcap: Rc<RefCell<PcapNgWriter<Vec<u8>>>>,
    tun: Rc<RefCell<Tunn>>,
    rx: Rc<RefCell<VecDeque<Vec<u8>>>>,
    socket: Rc<WebSocket>,
    socket_state: Rc<RefCell<WsConnectionState>>,
    mtu: usize,
    _reset_rate_limiter_interval: Rc<IntervalHandle<JsValue>>,
    _onopen_closure: Rc<Closure<dyn FnMut(JsValue)>>,
    _onclose_closure: Rc<Closure<dyn FnMut(JsValue)>>,
    _onerror_closure: Rc<Closure<dyn FnMut(JsValue)>>,
    _onmessage_closure: Rc<Closure<dyn FnMut(MessageEvent)>>,
}

impl WgTunDevice {

    /// Creates a new WgTunDevice and connects the underlaying Websocket
    pub fn new(
        self_key: x25519::StaticSecret,
        peer: x25519::PublicKey,
        psk: [u8; 32],
        url: &str,
        mtu: usize,
        disconnect_cb: js_sys::Function,
        connect_cb: js_sys::Function,
    ) -> Result<Self, JsValue> {
        let rate_limiter = Arc::new(RateLimiter::new(&x25519::PublicKey::from(&self_key), 10));

        let tun = Tunn::new(
            self_key,
            peer,
            Some(psk),
            Some(4),
            OsRng.try_next_u32().unwrap(),
            Some(rate_limiter.clone()),
        );
        let reset_rate_limiter = Closure::<dyn FnMut(_)>::new(move |_: JsValue| {
            rate_limiter.reset_count();
        });
        let _reset_rate_limiter_interval = Rc::new(IntervalHandle::new(reset_rate_limiter, 10000));

        let tun = Rc::new(RefCell::new(tun));
        let rx = Rc::new(RefCell::new(VecDeque::new()));
        let socket_state = Rc::new(RefCell::new(WsConnectionState::Disconnected));

        let socket = WebSocket::new(url)?;
        log::debug!("Parent WebSocket Created");

        let socket = Rc::new(socket);

        let onopen = create_onopen_closure(Rc::downgrade(&socket_state), Rc::downgrade(&tun), Rc::downgrade(&socket));
        let onopen = Rc::new(onopen);

        let onclose_socket_state = Rc::downgrade(&socket_state);
        let onclose = Closure::<dyn FnMut(_)>::new(move |_: JsValue| {
            log::debug!("Parent WebSocket Closed");
            *onclose_socket_state.upgrade().unwrap().borrow_mut() = WsConnectionState::Disconnected;
            let _ = disconnect_cb.call0(&JsValue::null());
        });
        let onclose = Rc::new(onclose);
        socket.set_onclose(Some(onclose.as_ref().as_ref().unchecked_ref()));

        let onerror_socket_state = Rc::downgrade(&socket_state);
        let onerror = Closure::<dyn FnMut(_)>::new(move |_: JsValue| {
            log::error!("Parent WebSocket Error");
            *onerror_socket_state.upgrade().unwrap().borrow_mut() = WsConnectionState::Disconnected;
        });
        let onerror = Rc::new(onerror);
        socket.set_onerror(Some(onerror.as_ref().as_ref().unchecked_ref()));

        let pcap = Rc::new(RefCell::new(new_pcap_writer()));

        let onmessage = create_onmessage_closure(Rc::downgrade(&socket), Rc::downgrade(&tun), Rc::downgrade(&rx), Rc::downgrade(&pcap), connect_cb);
        let onmessage = Rc::new(onmessage);
        socket.set_onmessage(Some(onmessage.as_ref().as_ref().unchecked_ref()));
        socket.set_onopen(Some(onopen.as_ref().as_ref().unchecked_ref()));

        Ok(Self {
            pcap,
            tun,
            rx,
            socket,
            socket_state,
            mtu,
            _reset_rate_limiter_interval,
            _onopen_closure: onopen,
            _onclose_closure: onclose,
            _onerror_closure: onerror,
            _onmessage_closure: onmessage,
        })
    }

    pub fn disconnect(&self) {
        *self.socket_state.borrow_mut() = WsConnectionState::Disconnected;
        self.socket.set_onopen(None);
        self.socket.set_onmessage(None);
        self.socket.set_onclose(None);
        self.socket.set_onerror(None);
        let _ = self.socket.close();
        self._reset_rate_limiter_interval.cancel();
    }

    pub fn get_pcap(&self) -> Rc<RefCell<PcapNgWriter<Vec<u8>>>> {
        self.pcap.clone()
    }
}

/// Creates the handler for the onClose event of the WebSocket
fn create_onopen_closure(
    onopen_socket_state: Weak<RefCell<WsConnectionState>>,
    onopen_tun: Weak<RefCell<Tunn>>,
    onopen_socket: Weak<WebSocket>,
) -> Closure<dyn FnMut(JsValue)> {
    Closure::<dyn FnMut(_)>::new(move |_: JsValue| {
        log::debug!("Parent WebSocket Opened");
        let onopen_socket_state = onopen_socket_state.upgrade().unwrap();
        *onopen_socket_state.borrow_mut() = WsConnectionState::Connected;

        let onopen_tun = onopen_tun.upgrade().unwrap();
        let mut tun = onopen_tun.borrow_mut();
        let mut buf = [0u8; 2048];
        match tun.format_handshake_initiation(&mut buf, false) {
            TunnResult::WriteToNetwork(d) => {
                log::debug!("Sending handshake initiation");
                let onopen_socket = onopen_socket.upgrade().unwrap();
                let _ = onopen_socket.send_with_u8_array(d);
            }
            _ => panic!("Unexpected TunnResult"),
        }
    })
}

fn new_pcap_writer() -> PcapNgWriter<Vec<u8>> {
    let mut writer = PcapNgWriter::new(Vec::new()).unwrap();
    writer.write_pcapng_block(InterfaceDescriptionBlock {
        // Medium::Ip carries both IP families without a link-layer header.
        linktype: pcap_file::DataLink::RAW,
        snaplen: 0,
        options: vec![],
    }).unwrap();
    writer
}

fn log_pcap_data(pcap: &Rc<RefCell<PcapNgWriter<Vec<u8>>>>, data: &[u8]) {
    let now = wasm_timer::SystemTime::now();

    // wasm_timer has a bug that returns time in milliseconds
    let timestamp = now.duration_since(wasm_timer::SystemTime::UNIX_EPOCH).unwrap() / 1000;
    let packet = pcap_file::pcapng::blocks::enhanced_packet::EnhancedPacketBlock {
        interface_id: 0,
        timestamp,
        original_len: data.len() as u32,
        data: std::borrow::Cow::Borrowed(data),
        options: vec![],
    };
    let mut writer = pcap.borrow_mut();
    writer.write_pcapng_block(packet).unwrap();
}

/// Creates the MessageEvent handler for the WebSocket onMessage event
fn create_onmessage_closure(
    message_socket: Weak<WebSocket>,
    message_tun: Weak<RefCell<Tunn>>,
    message_vec: Weak<RefCell<VecDeque<Vec<u8>>>>,
    message_pcap: Weak<RefCell<PcapNgWriter<Vec<u8>>>>,
    connect_cb: js_sys::Function,
) -> Closure<dyn FnMut(MessageEvent)> {
    let connected = Rc::new(RefCell::new(false));
    Closure::<dyn FnMut(_)>::new(move |e: MessageEvent| {
        let data = e.data();
        let data = match data.dyn_into::<web_sys::Blob>() {
            Ok(blob) => blob,
            Err(_) => {
                log::error!("Not a blob");
                return;
            }
        };

        let message_socket = message_socket.clone();
        let message_tun = message_tun.clone();
        let message_vec = message_vec.clone();
        let message_pcap = message_pcap.clone();
        let connected = connected.clone();
        let connect_cb = connect_cb.clone();
        spawn_local(async move {
            let array_buffer = wasm_bindgen_futures::JsFuture::from(data.array_buffer()).await.unwrap();
            let data = js_sys::Uint8Array::new(&array_buffer).to_vec();
            let (Some(message_tun), Some(socket), Some(queue), Some(pcap)) = (
                message_tun.upgrade(), message_socket.upgrade(), message_vec.upgrade(), message_pcap.upgrade(),
            ) else { return; };
            if socket.ready_state() != WebSocket::OPEN || data.is_empty() { return; }
            let result = receive_datagram(
                &mut message_tun.borrow_mut(), &data,
                |packet| {
                    if pcap_logging_enabled() { log_pcap_data(&pcap, packet); }
                    queue.borrow_mut().push_back(packet.to_vec());
                },
                |packet| { let _ = socket.send_with_u8_array(packet); },
            );
            match result {
                Ok(true) if !*connected.borrow() => {
                    *connected.borrow_mut() = true;
                    connect_cb.call0(&JsValue::null()).ok();
                }
                Err(e) => log::debug!("WireGuard receive error: {:?}", e),
                _ => (),
            }
        });
    })
}

/// Deliver plaintext to smoltcp. Only BoringTun's network results go back to
/// the relay; newly generated IP replies are encrypted by WgTunPhyTxToken.
/// After a network result, drain handshake/queued transport packets as required
/// by BoringTun's decapsulate API.
pub(crate) fn receive_datagram(
    tun: &mut Tunn, data: &[u8], mut deliver: impl FnMut(&[u8]), mut send: impl FnMut(&[u8]),
) -> Result<bool, WireGuardError> {
    let mut buf = vec![0; (u16::MAX as usize + 32).max(data.len() + 32)];
    let mut input = data;
    let mut wrote_network = false;
    loop {
        match tun.decapsulate(None, input, &mut buf) {
            TunnResult::Done => return Ok(wrote_network),
            TunnResult::Err(err) => return Err(err),
            TunnResult::WriteToNetwork(packet) => {
                send(packet);
                wrote_network = true;
                input = &[];
            }
            TunnResult::WriteToTunnelV4(packet, _) | TunnResult::WriteToTunnelV6(packet, _) => {
                deliver(packet);
                return Ok(wrote_network);
            }
        }
    }
}

pub trait IsUp {
    fn is_up(&self) -> bool;
}

impl IsUp for WgTunDevice {
    fn is_up(&self) -> bool {
        *self.socket_state.borrow() == WsConnectionState::Connected
            && self.tun.borrow().time_since_last_handshake().is_some()
    }
}

impl phy::Device for WgTunDevice {
    type RxToken<'a> = WgTunPhyRxToken;
    type TxToken<'a> = WgTunPhyTxToken;

    fn receive(&mut self, _: Instant) -> Option<(Self::RxToken<'_>, Self::TxToken<'_>)> {
        let mut deque = self.rx.borrow_mut();
        if *self.socket_state.borrow_mut() != WsConnectionState::Connected || deque.is_empty() {
            return None;
        }

        Some((
            WgTunPhyRxToken {
                // safe to unwrap because we checked for empty
                buf: deque.pop_front().unwrap(),
            },
            WgTunPhyTxToken {
                pcap: self.pcap.clone(),
                tun: self.tun.clone(),
                socket: self.socket.clone(),
            },
        ))
    }

    fn transmit(&mut self, _: Instant) -> Option<Self::TxToken<'_>> {
        if (*self.socket_state.borrow_mut()) != WsConnectionState::Connected {
            return None;
        }

        Some(WgTunPhyTxToken {
            pcap: self.pcap.clone(),
            tun: self.tun.clone(),
            socket: self.socket.clone(),
        })
    }

    fn capabilities(&self) -> DeviceCapabilities {
        let mut caps = DeviceCapabilities::default();
        caps.max_transmission_unit = self.mtu;
        caps.medium = Medium::Ip;
        caps
    }
}

pub struct WgTunPhyRxToken {
    buf: Vec<u8>,
}

impl phy::RxToken for WgTunPhyRxToken {
    fn consume<R, F>(mut self, f: F) -> R
    where
        F: FnOnce(&[u8]) -> R,
    {
        f(&mut self.buf[..])
    }
}

pub struct WgTunPhyTxToken {
    pcap: Rc<RefCell<PcapNgWriter<Vec<u8>>>>,
    tun: Rc<RefCell<Tunn>>,
    socket: Rc<WebSocket>,
}

impl phy::TxToken for WgTunPhyTxToken {
    fn consume<R, F>(self, size: usize, f: F) -> R
    where
        F: FnOnce(&mut [u8]) -> R,
    {
        let mut buf = vec![0u8; size];
        let result = f(&mut buf[..]);

        if pcap_logging_enabled() {
            log_pcap_data(&self.pcap, &buf);
        }

        let mut tun = self.tun.borrow_mut();
        let mut dst_buf = vec![0u8; (size + 32).max(148)];
        match tun.encapsulate(&buf, &mut dst_buf) {
            TunnResult::Done => (),
            TunnResult::WriteToNetwork(d) => {
                let _ = self.socket.send_with_u8_array(d);
            }
            TunnResult::Err(e) => {
                log::error!("Error in recv: {:?}", e);
                return result;
            }
            _ => {
                log::error!("Unknown TunnResult");
                return result;
            }
        }

        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pair() -> (Tunn, Tunn) {
        let a = x25519::StaticSecret::from([1; 32]);
        let b = x25519::StaticSecret::from([2; 32]);
        let ap = x25519::PublicKey::from(&a);
        let bp = x25519::PublicKey::from(&b);
        (Tunn::new(a, bp, Some([3; 32]), None, 1, None),
         Tunn::new(b, ap, Some([3; 32]), None, 2, None))
    }

    fn ip_packets() -> Vec<Vec<u8>> {
        let mut v4 = vec![0; 20];
        v4[0] = 0x45; v4[2..4].copy_from_slice(&20u16.to_be_bytes());
        v4[12..16].copy_from_slice(&[10, 123, 1, 2]);
        v4[16..20].copy_from_slice(&[10, 123, 1, 3]);
        let mut v6 = vec![0; 40]; v6[0] = 0x60; v6[6] = 59;
        v6[8..24].copy_from_slice(&"fd00::2".parse::<std::net::Ipv6Addr>().unwrap().octets());
        v6[24..40].copy_from_slice(&"fd00::3".parse::<std::net::Ipv6Addr>().unwrap().octets());
        vec![v4, v6]
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    fn pcap_records_both_families_on_one_raw_ip_interface() {
        use pcap_file::pcapng::{Block, PcapNgReader};

        let pcap = Rc::new(RefCell::new(new_pcap_writer()));
        let packets = ip_packets();
        for packet in &packets {
            log_pcap_data(&pcap, packet);
        }
        let writer = pcap.borrow();
        let mut reader = PcapNgReader::new(writer.get_ref().as_slice()).unwrap();
        let Some(Ok(Block::InterfaceDescription(interface))) = reader.next_block() else {
            panic!("missing capture interface");
        };
        assert_eq!(interface.linktype, pcap_file::DataLink::RAW);
        for expected in packets {
            let Some(Ok(Block::EnhancedPacket(packet))) = reader.next_block() else {
                panic!("expected an IP packet, without an extra interface descriptor");
            };
            assert_eq!(packet.interface_id, 0);
            assert_eq!(packet.original_len as usize, expected.len());
            assert_eq!(packet.data.as_ref(), expected);
        }
        assert!(reader.next_block().is_none());
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    fn receives_both_families_without_retransmitting_encrypted_input() {
        let (mut browser, mut peer) = pair();
        let mut buf = [0; 2048];
        let TunnResult::WriteToNetwork(init) = browser.format_handshake_initiation(&mut buf, false) else { panic!() };
        let mut responses = Vec::new();
        receive_datagram(&mut peer, init, |_| panic!("handshake is not IP"), |p| responses.push(p.to_vec())).unwrap();
        let mut keepalives = Vec::new();
        receive_datagram(&mut browser, &responses[0], |_| panic!(), |p| keepalives.push(p.to_vec())).unwrap();
        receive_datagram(&mut peer, &keepalives[0], |_| panic!(), |_| panic!()).unwrap();
        for packet in ip_packets() {
            let TunnResult::WriteToNetwork(encrypted) = peer.encapsulate(&packet, &mut buf) else { panic!() };
            let mut received = VecDeque::new();
            let mut sent = Vec::new();
            receive_datagram(&mut browser, encrypted, |p| received.push_back(p.to_vec()), |p| sent.push(p.to_vec())).unwrap();
            assert!(sent.is_empty(), "data receipt retransmitted an encrypted datagram");
            let token = WgTunPhyRxToken { buf: received.pop_front().unwrap() };
            smoltcp::phy::RxToken::consume(token, |p| assert_eq!(p, packet));
            assert!(received.is_empty());
        }
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    fn drains_handshake_keepalive_and_queued_ip() {
        let (mut browser, mut peer) = pair();
        let packet = ip_packets().remove(1);
        let mut buf = [0; 2048];
        let TunnResult::WriteToNetwork(init) = browser.encapsulate(&packet, &mut buf) else { panic!() };
        let mut responses = Vec::new();
        receive_datagram(&mut peer, init, |_| panic!(), |p| responses.push(p.to_vec())).unwrap();
        let mut output = Vec::new();
        receive_datagram(&mut browser, &responses[0], |_| panic!(), |p| output.push(p.to_vec())).unwrap();
        assert_eq!(output.len(), 2, "handshake completion must drain the queued IP packet");
        let mut delivered = Vec::new();
        for datagram in output {
            receive_datagram(&mut peer, &datagram, |p| delivered.push(p.to_vec()), |_| panic!()).unwrap();
        }
        assert_eq!(delivered, vec![packet]);
    }
}
