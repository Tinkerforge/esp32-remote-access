use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use gloo_file::Blob;
use smoltcp::phy::{self, DeviceCapabilities, Medium};
use smoltcp::time::Instant;
use rand_core::{OsRng, RngCore};
use boringtun::{
    noise::{Tunn, TunnResult},
    x25519
};
use wasm_bindgen_futures::spawn_local;
use web_sys::wasm_bindgen::{JsValue, JsCast};
use web_sys::{WebSocket, MessageEvent};
use web_sys::wasm_bindgen::closure::Closure;

#[derive(PartialEq)]
enum WsConnectionState {
    Connected,
    Disconnected,
}

pub struct WgTunDevice {
    tun: Arc<Mutex<Tunn>>,
    rx: Arc<Mutex<VecDeque<Vec<u8>>>>,
    socket: Arc<WebSocket>,
    socket_state: Arc<Mutex<WsConnectionState>>,
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

        let tun = Arc::new(Mutex::new(tun));
        let rx = Arc::new(Mutex::new(VecDeque::new()));
        let socket_state = Arc::new(Mutex::new(WsConnectionState::Disconnected));

        let socket = WebSocket::new("ws://localhost:8081")?;
        log::info!("Parent WebSocket Created");

        let socket = Arc::new(socket);
        let onopen_socket = socket.clone();
        let onopen_socket_state = socket_state.clone();
        let onopen_tun = tun.clone();
        let onopen = Closure::<dyn FnMut(_)>::new(move |_: MessageEvent| {
            log::info!("Parent WebSocket Opened");
            *onopen_socket_state.lock().unwrap() = WsConnectionState::Connected;

            let mut tun = onopen_tun.lock().unwrap();
            let mut buf = [0u8; 2048];
            match tun.format_handshake_initiation(&mut buf, false) {
                TunnResult::WriteToNetwork(d) => {
                    log::info!("Sending handshake initiation");
                    let _ = onopen_socket.send_with_u8_array(d);
                },
                _ => panic!("Unexpected TunnResult")
            }
        });

        let onclose_socket_state = socket_state.clone();
        let onclose = Closure::<dyn FnMut(_)>::new(move |_: MessageEvent| {
            log::info!("Parent WebSocket Closed");
            *onclose_socket_state.lock().unwrap() = WsConnectionState::Disconnected;
        });
        socket.set_onclose(Some(onclose.as_ref().unchecked_ref()));

        let onerror_socket_state = socket_state.clone();
        let onerror = Closure::<dyn FnMut(_)>::new(move |_: MessageEvent| {
            log::info!("Parent WebSocket Error");
            *onerror_socket_state.lock().unwrap() = WsConnectionState::Disconnected;
        });
        socket.set_onerror(Some(onerror.as_ref().unchecked_ref()));

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

            let loaded = Closure::<dyn FnMut(_)>::new(move |_: JsValue| {
                let array = js_sys::Uint8Array::new(&fr_c.result().unwrap());
                let data = array.to_vec();
                let mut tun = message_tun.lock().unwrap();
                if data.is_empty() {
                    log::info!("Empty data");
                    return
                }

                let mut buf = vec![0u8; data.len() + 32];
                match tun.decapsulate(None, &data, &mut buf) {
                    TunnResult::Done => (),
                    TunnResult::WriteToNetwork(d) => {
                        log::info!("Sending data to tun");
                        let _ = message_socket.send_with_u8_array(d);
                        let mut buf = [0u8; 2048];
                        while let tun.decapsulate(None, &[0u8; 0], &mut buf)
                        return;
                    },
                    TunnResult::Err(e) => {
                        log::error!("Error: {:?}", e);
                        return
                    },
                    TunnResult::WriteToTunnelV4(data, ip) => {
                        log::info!("Sending data to {}", ip);
                        log::info!("Data: {:?}", data);
                        let mut buf = vec![0u8; data.len() + 32];
                        match tun.encapsulate(&data, &mut buf) {
                            TunnResult::WriteToNetwork(d) => {
                                log::info!("Writing data to tun");
                                let _ = message_socket.send_with_u8_array(d);
                                return;
                            },
                            _ => panic!("Unexpected TunnResult")
                        }
                    },
                    _ => {
                        log::error!("Unknown TunnResult");
                        return
                    }
                }
                log::info!("Received data from tun, {:?}", buf);
                (*message_vec.lock().unwrap()).push_back(buf.to_vec());
            });

            fr.set_onload(Some(loaded.as_ref().unchecked_ref()));
            fr.read_as_array_buffer(&data).unwrap();
            loaded.forget();

        });
        socket.set_onmessage(Some(onmessage.as_ref().unchecked_ref()));
        socket.set_onopen(Some(onopen.as_ref().as_ref().unchecked_ref()));

        let keepalive_socket = socket.clone();
        let keepalive_socket_state = socket_state.clone();
        let keepalive_tun = tun.clone();
        let keepalive = Closure::<dyn FnMut(_)>::new(move |_: MessageEvent| {
            if *keepalive_socket_state.lock().unwrap() != WsConnectionState::Connected {
                return
            }
            log::info!("Keepalive");
            let mut tun = keepalive_tun.lock().unwrap();
            let mut buf = [0u8; 1500];
            let mut buf2 = [0u8; 2048];
            log::info!("Stats: {:?}", tun.stats());
            match tun.encapsulate(&mut buf, &mut buf2) {
                TunnResult::WriteToNetwork(d) => {
                    log::info!("Sending keepalive");
                    let _ = keepalive_socket.send_with_u8_array(d);
                },
                TunnResult::Done => return,
                _ => panic!("Unexpected TunnResult")
            }
            keepalive_socket.send_with_u8_array(&buf2).unwrap();
        });

        // let window = web_sys::window().unwrap();
        // window.set_interval_with_callback_and_timeout_and_arguments_0(
        //     keepalive.as_ref().unchecked_ref(),
        //     5000).unwrap();

        // !!!! This leaks memory !!!!
        // But it should be fine because the Object should have a static lifetime
        onclose.forget();
        onopen.forget();
        onerror.forget();
        onmessage.forget();
        keepalive.forget();


        Ok(Self {
            tun,
            rx,
            socket,
            socket_state,
        })
    }
}

impl phy::Device for WgTunDevice {
    type RxToken<'a> = WgTunPhyRxToken;
    type TxToken<'a> = WgTunPhyTxToken;

    fn receive(&mut self, _: Instant) -> Option<(Self::RxToken<'_>, Self::TxToken<'_>)> {
        let mut deque = self.rx.lock().unwrap();
        if *self.socket_state.lock().unwrap() != WsConnectionState::Connected || deque.is_empty() {
            return None
        }

        Some((
            WgTunPhyRxToken {
                // safe to unwrap because we checked for empty
                buf: deque.pop_front().unwrap(),
            },
            WgTunPhyTxToken {
                tun: self.tun.clone(),
                socket: self.socket.clone(),
            },
        ))
    }

    fn transmit(&mut self, _: Instant) -> Option<Self::TxToken<'_>> {
        if (*self.socket_state.lock().unwrap()) != WsConnectionState::Connected {
            return None
        }

        Some(WgTunPhyTxToken {
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
    tun: Arc<Mutex<Tunn>>,
    socket: Arc<WebSocket>,
}

impl phy::TxToken for WgTunPhyTxToken {
    fn consume<R, F>(self, size: usize, f: F) -> R
        where
            F: FnOnce(&mut [u8]) -> R
    {
        let mut buf = vec![0u8; size];
        let result = f(&mut buf[..]);

        let mut tun = self.tun.lock().unwrap();
        let mut dst_buf = vec![0u8; size + 32];
        match tun.encapsulate(&buf, &mut dst_buf) {
            TunnResult::Done => (),
            TunnResult::WriteToNetwork(d) => {
                let _ = self.socket.send_with_u8_array(d);
            },
            TunnResult::Err(e) => {
                log::error!("Error: {:?}", e);
                return result
            },
            _ => {
                log::error!("Unknown TunnResult");
                return result
            }
        }

        result
    }
}
