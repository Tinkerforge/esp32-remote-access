use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;
use pcap_file::pcapng::PcapNgWriter;
use pcap_file::pcapng::blocks::interface_description::InterfaceDescriptionBlock;
use smoltcp::phy::{self, DeviceCapabilities, Medium};
use smoltcp::time::Instant;
use rand_core::{OsRng, RngCore};
use boringtun::{
    noise::{Tunn, TunnResult},
    x25519
};

use crate::console_log;
use web_sys::wasm_bindgen::{JsValue, JsCast};
use web_sys::{WebSocket, MessageEvent};
use web_sys::wasm_bindgen::closure::Closure;

#[derive(PartialEq)]
enum WsConnectionState {
    Connected,
    Disconnected,
}

pub struct WgTunDevice {
    pcap: Rc<RefCell<PcapNgWriter<Vec<u8>>>>,
    tun: Rc<RefCell<Tunn>>,
    rx: Rc<RefCell<VecDeque<Vec<u8>>>>,
    socket: Rc<WebSocket>,
    socket_state: Rc<RefCell<WsConnectionState>>,
}

impl WgTunDevice {
    pub fn new(self_key: x25519::StaticSecret, peer: x25519::PublicKey) -> Result<Self, JsValue> {
        let tun = Tunn::new(
            self_key,
            peer,
            None,
            Some(4),
            OsRng.next_u32(),
            None
        );

        let tun = Rc::new(RefCell::new(tun));
        let rx = Rc::new(RefCell::new(VecDeque::new()));
        let socket_state = Rc::new(RefCell::new(WsConnectionState::Disconnected));

        let socket = WebSocket::new("ws://192.168.1.215:8081")?;
        console_log!("Parent WebSocket Created");

        let socket = Rc::new(socket);
        let onopen_socket = socket.clone();
        let onopen_socket_state = socket_state.clone();
        let onopen_tun = tun.clone();
        let onopen = Closure::<dyn FnMut(_)>::new(move |_: MessageEvent| {
            console_log!("Parent WebSocket Opened");
            *onopen_socket_state.borrow_mut() = WsConnectionState::Connected;

            let mut tun = onopen_tun.borrow_mut();
            let mut buf = [0u8; 2048];
            match tun.format_handshake_initiation(&mut buf, false) {
                TunnResult::WriteToNetwork(d) => {
                    console_log!("Sending handshake initiation");
                    let _ = onopen_socket.send_with_u8_array(d);
                },
                _ => panic!("Unexpected TunnResult")
            }
        });

        let onclose_socket_state = socket_state.clone();
        let onclose = Closure::<dyn FnMut(_)>::new(move |_: MessageEvent| {
            console_log!("Parent WebSocket Closed");
            *onclose_socket_state.borrow_mut() = WsConnectionState::Disconnected;
        });
        socket.set_onclose(Some(onclose.as_ref().unchecked_ref()));

        let onerror_socket_state = socket_state.clone();
        let onerror = Closure::<dyn FnMut(_)>::new(move |_: MessageEvent| {
            console_log!("Parent WebSocket Error");
            *onerror_socket_state.borrow_mut() = WsConnectionState::Disconnected;
        });
        socket.set_onerror(Some(onerror.as_ref().unchecked_ref()));

        let pcap = vec![];
        let pcap = Rc::new(RefCell::new(PcapNgWriter::new(pcap).unwrap()));

        let message_pcap = pcap.clone();
        let message_vec = rx.clone();
        let message_socket = socket.clone();
        let message_tun = tun.clone();
        let onmessage = Closure::<dyn FnMut(_)>::new(move |e: MessageEvent| {
            let data = e.data();
            let data = data.dyn_into::<web_sys::Blob>().unwrap();

            let fr = web_sys::FileReader::new().unwrap();
            let fr_c = fr.clone();
            let message_tun = message_tun.clone();
            let message_socket = message_socket.clone();
            let message_vec = message_vec.clone();
            let message_pcap = message_pcap.clone();

            let loaded = Closure::<dyn FnMut(_)>::new(move |_: JsValue| {
                let array = js_sys::Uint8Array::new(&fr_c.result().unwrap());
                let data = array.to_vec();
                let mut tun = message_tun.borrow_mut();
                if data.is_empty() {
                    console_log!("Empty data");
                    return
                }

                let mut buf = vec![0u8; data.len() + 32];
                match tun.decapsulate(None, &data, &mut buf) {
                    TunnResult::Done => (),
                    TunnResult::WriteToNetwork(d) => {
                        let interface = InterfaceDescriptionBlock {
                            linktype: pcap_file::DataLink::IPV4,
                            snaplen: 0,
                            options: vec![],
                        };

                        let now = wasm_timer::SystemTime::now();
                        let timestamp = now.duration_since(wasm_timer::SystemTime::UNIX_EPOCH).unwrap();

                        let packet = pcap_file::pcapng::blocks::enhanced_packet::EnhancedPacketBlock {
                            interface_id: 0,
                            timestamp,
                            original_len: d.len() as u32,
                            data: std::borrow::Cow::Borrowed(&d),
                            options: vec![],
                        };

                        {
                            let mut message_pcap = message_pcap.borrow_mut();
                            message_pcap.write_pcapng_block(interface).unwrap();
                            message_pcap.write_pcapng_block(packet).unwrap();
                        }

                        let _ = message_socket.send_with_u8_array(d);
                        let mut buf = [0u8; 2048];

                        /*
                         * If the result is of type TunnResult::WriteToNetwork, should repeat the call with empty datagram,
                         * until TunnResult::Done is returned. If batch processing packets,
                         * it is OK to defer until last packet is processed.
                         *
                         * From Tunn::decapsulate.
                         */
                        while let TunnResult::WriteToNetwork(d) = tun.decapsulate(None, &[0u8; 0], &mut buf) {
                            let interface = InterfaceDescriptionBlock {
                                linktype: pcap_file::DataLink::IPV4,
                                snaplen: 0,
                                options: vec![],
                            };
                            let now = wasm_timer::SystemTime::now();
                            let timestamp = now.duration_since(wasm_timer::SystemTime::UNIX_EPOCH).unwrap();

                            let packet = pcap_file::pcapng::blocks::enhanced_packet::EnhancedPacketBlock {
                                interface_id: 0,
                                timestamp,
                                original_len: d.len() as u32,
                                data: std::borrow::Cow::Borrowed(&d),
                                options: vec![],
                            };

                            {
                                let mut message_pcap = message_pcap.borrow_mut();
                                message_pcap.write_pcapng_block(interface).unwrap();
                                message_pcap.write_pcapng_block(packet).unwrap();
                            }
                            let _ = message_socket.send_with_u8_array(d);
                        }
                        return;
                    },
                    TunnResult::Err(e) => {
                        console_log!("Error: {:?}", e);
                        return
                    },
                    TunnResult::WriteToTunnelV4(d, _) => {
                        let interface = InterfaceDescriptionBlock {
                            linktype: pcap_file::DataLink::IPV4,
                            snaplen: 0,
                            options: vec![],
                        };
                        let now = wasm_timer::SystemTime::now();
                        let timestamp = now.duration_since(wasm_timer::SystemTime::UNIX_EPOCH).unwrap();

                        let packet = pcap_file::pcapng::blocks::enhanced_packet::EnhancedPacketBlock {
                            interface_id: 0,
                            timestamp,
                            original_len: d.len() as u32,
                            data: std::borrow::Cow::Borrowed(&d),
                            options: vec![],
                        };

                        {
                            let mut message_pcap = message_pcap.borrow_mut();
                            message_pcap.write_pcapng_block(interface).unwrap();
                            message_pcap.write_pcapng_block(packet).unwrap();
                        }
                        (*message_vec.borrow_mut()).push_back(d.to_vec());
                        let mut buf = vec![0u8; data.len() + 32];
                        match tun.encapsulate(&data, &mut buf) {
                            TunnResult::WriteToNetwork(d) => {
                                let _ = message_socket.send_with_u8_array(d);
                                return;
                            },
                            _ => panic!("Unexpected TunnResult")
                        }
                    },
                    _ => {
                        console_log!("Unknown TunnResult");
                        return
                    }
                }
                let interface = InterfaceDescriptionBlock {
                    linktype: pcap_file::DataLink::IPV4,
                    snaplen: 0,
                    options: vec![],
                };
                let now = wasm_timer::SystemTime::now();
                let timestamp = now.duration_since(wasm_timer::SystemTime::UNIX_EPOCH).unwrap();

                let packet = pcap_file::pcapng::blocks::enhanced_packet::EnhancedPacketBlock {
                    interface_id: 0,
                    timestamp,
                    original_len: buf.len() as u32,
                    data: std::borrow::Cow::Borrowed(&buf),
                    options: vec![],
                };

                {
                    let mut message_pcap = message_pcap.borrow_mut();
                    message_pcap.write_pcapng_block(interface).unwrap();
                    message_pcap.write_pcapng_block(packet).unwrap();
                }
                (*message_vec.borrow_mut()).push_back(buf.to_vec());
            });

            fr.set_onload(Some(loaded.as_ref().unchecked_ref()));
            fr.read_as_array_buffer(&data).unwrap();
            loaded.forget();

        });
        socket.set_onmessage(Some(onmessage.as_ref().unchecked_ref()));
        socket.set_onopen(Some(onopen.as_ref().as_ref().unchecked_ref()));

        // !!!! This leaks memory !!!!
        // But it should be fine because the Object should have a static lifetime
        onclose.forget();
        onopen.forget();
        onerror.forget();
        onmessage.forget();

        Ok(Self {
            pcap,
            tun,
            rx,
            socket,
            socket_state,
        })
    }

    pub fn get_pcap(&self) -> Rc<RefCell<PcapNgWriter<Vec<u8>>>> {
        self.pcap.clone()
    }
}

impl phy::Device for WgTunDevice {
    type RxToken<'a> = WgTunPhyRxToken;
    type TxToken<'a> = WgTunPhyTxToken;

    fn receive(&mut self, _: Instant) -> Option<(Self::RxToken<'_>, Self::TxToken<'_>)> {
        let mut deque = self.rx.borrow_mut();
        if *self.socket_state.borrow_mut() != WsConnectionState::Connected || deque.is_empty() {
            return None
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
            return None
        }

        Some(WgTunPhyTxToken {
            pcap: self.pcap.clone(),
            tun: self.tun.clone(),
            socket: self.socket.clone(),
        })
    }

    fn capabilities(&self) -> DeviceCapabilities {
        let mut caps = DeviceCapabilities::default();
        caps.max_transmission_unit = 1500;
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
            F: FnOnce(&mut [u8]) -> R
    {
        let mut buf = vec![0u8; size];
        let result = f(&mut buf[..]);

        let interface = InterfaceDescriptionBlock {
            linktype: pcap_file::DataLink::IPV4,
            snaplen: 0,
            options: vec![],
        };
        let now = wasm_timer::SystemTime::now();
        let timestamp = now.duration_since(wasm_timer::SystemTime::UNIX_EPOCH).unwrap();

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

        let mut tun = self.tun.borrow_mut();
        let mut dst_buf = vec![0u8; size + 32];
        match tun.encapsulate(&buf, &mut dst_buf) {
            TunnResult::Done => (),
            TunnResult::WriteToNetwork(d) => {
                let _ = self.socket.send_with_u8_array(d);
            },
            TunnResult::Err(e) => {
                console_log!("Error: {:?}", e);
                return result
            },
            _ => {
                console_log!("Unknown TunnResult");
                return result
            }
        }

        result
    }
}
