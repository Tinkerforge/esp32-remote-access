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
use rand_core::{OsRng, RngCore};
use smoltcp::phy::{self, DeviceCapabilities, Medium};
use smoltcp::time::Instant;
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
    _reset_rate_limiter_interval: Rc<IntervalHandle<JsValue>>,
    _onopen_closure: Rc<Closure<dyn FnMut(JsValue) -> ()>>,
    _onclose_closure: Rc<Closure<dyn FnMut(JsValue) -> ()>>,
    _onerror_closure: Rc<Closure<dyn FnMut(JsValue) -> ()>>,
    _onmessage_closure: Rc<Closure<dyn FnMut(MessageEvent) -> ()>>,
}

impl WgTunDevice {

    /// Creates a new WgTunDevice and connects the underlaying Websocket
    pub fn new(
        self_key: x25519::StaticSecret,
        peer: x25519::PublicKey,
        psk: [u8; 32],
        url: &str,
        disconnect_cb: js_sys::Function,
    ) -> Result<Self, JsValue> {
        let rate_limiter = Arc::new(RateLimiter::new(&x25519::PublicKey::from(&self_key), 10));

        let tun = Tunn::new(
            self_key,
            peer,
            Some(psk),
            Some(4),
            OsRng.next_u32(),
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

        let pcap = vec![];
        let pcap = Rc::new(RefCell::new(PcapNgWriter::new(pcap).unwrap()));

        let onmessage = create_onmessage_closure(Rc::downgrade(&socket), Rc::downgrade(&tun), Rc::downgrade(&rx), Rc::downgrade(&pcap));
        let onmessage = Rc::new(onmessage);
        socket.set_onmessage(Some(onmessage.as_ref().as_ref().unchecked_ref()));
        socket.set_onopen(Some(onopen.as_ref().as_ref().unchecked_ref()));

        Ok(Self {
            pcap,
            tun,
            rx,
            socket,
            socket_state,
            _reset_rate_limiter_interval,
            _onopen_closure: onopen,
            _onclose_closure: onclose,
            _onerror_closure: onerror,
            _onmessage_closure: onmessage,
        })
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
) -> Closure<dyn FnMut(JsValue) -> ()> {
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

fn create_onmessage_closure(
    message_socket: Weak<WebSocket>,
    message_tun: Weak<RefCell<Tunn>>,
    message_vec: Weak<RefCell<VecDeque<Vec<u8>>>>,
    message_pcap: Weak<RefCell<PcapNgWriter<Vec<u8>>>>,
) -> Closure<dyn FnMut(MessageEvent) -> ()> {
    Closure::<dyn FnMut(_)>::new(move |e: MessageEvent| {
        let data = e.data();
        let data = match data.dyn_into::<web_sys::Blob>() {
            Ok(blob) => blob,
            Err(_) => {
                log::error!("Not a blob");
                return;
            }
        };

        let fr = web_sys::FileReader::new().unwrap();
        let fr_cpy = fr.clone();
        let message_tun = message_tun.clone();
        let message_socket = message_socket.clone();
        let message_vec = message_vec.clone();
        let message_pcap = message_pcap.clone();

        let loaded = Closure::<dyn FnMut(_)>::new(move |_: JsValue| {
            let value = match fr_cpy.result() {
                Ok(v) => v,
                Err(_) => {
                    log::error!("Error reading file");
                    return;
                }
            };
            let array = js_sys::Uint8Array::new(&value);
            let data = array.to_vec();
            let message_tun = message_tun.upgrade().unwrap();
            let mut tun = message_tun.borrow_mut();
            if data.is_empty() {
                log::error!("Empty data");
                return;
            }

            let mut buf = vec![0u8; data.len() + 32];
            match tun.decapsulate(None, &data, &mut buf) {
                TunnResult::Done => (),
                TunnResult::WriteToNetwork(d) => {
                    if pcap_logging_enabled() {
                        let interface = InterfaceDescriptionBlock {
                            linktype: pcap_file::DataLink::IPV4,
                            snaplen: 0,
                            options: vec![],
                        };

                        let now = wasm_timer::SystemTime::now();
                        let timestamp = now
                            .duration_since(wasm_timer::SystemTime::UNIX_EPOCH)
                            .unwrap();

                        let packet =
                            pcap_file::pcapng::blocks::enhanced_packet::EnhancedPacketBlock {
                                interface_id: 0,
                                timestamp,
                                original_len: d.len() as u32,
                                data: std::borrow::Cow::Borrowed(&d),
                                options: vec![],
                            };

                        {
                            let message_pcap = message_pcap.upgrade().unwrap();
                            let mut message_pcap = message_pcap.borrow_mut();
                            message_pcap.write_pcapng_block(interface).unwrap();
                            message_pcap.write_pcapng_block(packet).unwrap();
                        }
                    }
                    let message_socket = message_socket.upgrade().unwrap();
                    let _ = message_socket.send_with_u8_array(d);
                    let mut buf = [0u8; 2048];

                    /*
                     * If the result is of type TunnResult::WriteToNetwork, should repeat the call with empty datagram,
                     * until TunnResult::Done is returned. If batch processing packets,
                     * it is OK to defer until last packet is processed.
                     *
                     * From Tunn::decapsulate.
                     */
                    while let TunnResult::WriteToNetwork(d) =
                        tun.decapsulate(None, &[0u8; 0], &mut buf)
                    {
                        if pcap_logging_enabled() {
                            let interface = InterfaceDescriptionBlock {
                                linktype: pcap_file::DataLink::IPV4,
                                snaplen: 0,
                                options: vec![],
                            };
                            let now = wasm_timer::SystemTime::now();
                            let timestamp = now
                                .duration_since(wasm_timer::SystemTime::UNIX_EPOCH)
                                .unwrap();

                            let packet =
                                pcap_file::pcapng::blocks::enhanced_packet::EnhancedPacketBlock {
                                    interface_id: 0,
                                    timestamp,
                                    original_len: d.len() as u32,
                                    data: std::borrow::Cow::Borrowed(&d),
                                    options: vec![],
                                };

                            {
                                let message_pcap = message_pcap.upgrade().unwrap();
                                let mut message_pcap = message_pcap.borrow_mut();
                                message_pcap.write_pcapng_block(interface).unwrap();
                                message_pcap.write_pcapng_block(packet).unwrap();
                            }
                        }
                        let _ = message_socket.send_with_u8_array(d);
                    }
                    return;
                }
                TunnResult::Err(e) => {
                    if let WireGuardError::InvalidPacket = e {
                        log::debug!("Invalid packet");
                    } else {
                        log::debug!("Error: {:?}", e);
                    }
                    return;
                }
                TunnResult::WriteToTunnelV4(d, _) => {
                    if pcap_logging_enabled() {
                        let interface = InterfaceDescriptionBlock {
                            linktype: pcap_file::DataLink::IPV4,
                            snaplen: 0,
                            options: vec![],
                        };
                        let now = wasm_timer::SystemTime::now();
                        let timestamp = now
                            .duration_since(wasm_timer::SystemTime::UNIX_EPOCH)
                            .unwrap();

                        let packet =
                            pcap_file::pcapng::blocks::enhanced_packet::EnhancedPacketBlock {
                                interface_id: 0,
                                timestamp,
                                original_len: d.len() as u32,
                                data: std::borrow::Cow::Borrowed(&d),
                                options: vec![],
                            };

                        {
                            let message_pcap = message_pcap.upgrade().unwrap();
                            let mut message_pcap = message_pcap.borrow_mut();
                            message_pcap.write_pcapng_block(interface).unwrap();
                            message_pcap.write_pcapng_block(packet).unwrap();
                        }
                    }
                    let message_vec = message_vec.upgrade().unwrap();
                    (*message_vec.borrow_mut()).push_back(d.to_vec());
                    let mut buf = vec![0u8; data.len() + 32];
                    match tun.encapsulate(&data, &mut buf) {
                        TunnResult::WriteToNetwork(d) => {
                            drop(tun);
                            let message_socket = message_socket.upgrade().unwrap();
                            let _ = message_socket.send_with_u8_array(d);
                            return;
                        }
                        _ => panic!("Unexpected TunnResult"),
                    }
                }
                _ => {
                    log::error!("Unknown TunnResult");
                    return;
                }
            }
            if pcap_logging_enabled() {
                let interface = InterfaceDescriptionBlock {
                    linktype: pcap_file::DataLink::IPV4,
                    snaplen: 0,
                    options: vec![],
                };
                let now = wasm_timer::SystemTime::now();
                let timestamp = now
                    .duration_since(wasm_timer::SystemTime::UNIX_EPOCH)
                    .unwrap();

                let packet = pcap_file::pcapng::blocks::enhanced_packet::EnhancedPacketBlock {
                    interface_id: 0,
                    timestamp,
                    original_len: buf.len() as u32,
                    data: std::borrow::Cow::Borrowed(&buf),
                    options: vec![],
                };

                {
                    let message_pcap = message_pcap.upgrade().unwrap();
                    let mut message_pcap = message_pcap.borrow_mut();
                    message_pcap.write_pcapng_block(interface).unwrap();
                    message_pcap.write_pcapng_block(packet).unwrap();
                }
            }
            let message_vec = message_vec.upgrade().unwrap();
            (*message_vec.borrow_mut()).push_back(buf.to_vec());
        });

        fr.set_onload(Some(loaded.as_ref().unchecked_ref()));
        fr.read_as_array_buffer(&data).unwrap();

        // FIXME: need to get rid of this since it leaks memory
        loaded.forget();
    })
}

pub trait IsUp {
    fn is_up(&self) -> bool;
}

impl IsUp for WgTunDevice {
    fn is_up(&self) -> bool {
        self.tun.borrow().time_since_last_handshake().is_some()
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
        caps.max_transmission_unit = 1392;
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
        F: FnOnce(&mut [u8]) -> R,
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
            let interface = InterfaceDescriptionBlock {
                linktype: pcap_file::DataLink::IPV4,
                snaplen: 0,
                options: vec![],
            };
            let now = wasm_timer::SystemTime::now();
            let timestamp = now
                .duration_since(wasm_timer::SystemTime::UNIX_EPOCH)
                .unwrap();

            let packet = pcap_file::pcapng::blocks::enhanced_packet::EnhancedPacketBlock {
                interface_id: 0,
                timestamp,
                original_len: buf.len() as u32,
                data: std::borrow::Cow::Borrowed(&buf),
                options: vec![],
            };

            {
                let mut message_pcap = self.pcap.borrow_mut();
                message_pcap.write_pcapng_block(interface).unwrap();
                message_pcap.write_pcapng_block(packet).unwrap();
            }
        }

        let mut tun = self.tun.borrow_mut();
        let mut dst_buf = vec![0u8; size + 32];
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
pub mod test {
    use super::*;
    use wasm_bindgen_test::*;

    fn create_wg_tun_device() -> WgTunDevice {
        WgTunDevice::new(
            x25519::StaticSecret::random_from_rng(rand_core::OsRng),
            x25519::PublicKey::from(&x25519::StaticSecret::random_from_rng(rand_core::OsRng)),
            [0u8; 32],
            "ws://localhost:8082",
                js_sys::Function::new_no_args(""),
        )
        .unwrap()
    }

    #[wasm_bindgen_test]
    fn test_create_wg_tun_device() {
        let _ = create_wg_tun_device();
    }
}
