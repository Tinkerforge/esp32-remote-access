
use std::{io::{Write, Read}, sync::{Arc, Mutex}};

use http::request;
use smoltcp::wire::{IpAddress, IpCidr};
use wasm_bindgen::prelude::*;
use web_sys::MessageEvent;
use crate::{stream::{TcpStream, self}, wg_device::WgTunDevice, handshake::RequestWrap};
use base64::Engine;
use boringtun::x25519;

use crate::console_log;

#[wasm_bindgen]
pub struct WgClient {
    stream: Arc<Mutex<TcpStream<'static, WgTunDevice>>>
}

const SECRET: &str = "EMx11sTpRVrReWObruImxwm3rxZMwSJWBqdIJRDPxHM=";
const PEER: &str = "AZmudADBwjZIF6vOEDnnzgVPmg/hI987RPllAM1wW2w=";

#[wasm_bindgen]
impl WgClient {
    #[wasm_bindgen(constructor)]
    pub fn new(secret_str: &str, peer_str: &str) -> Self {

        let mut secret = [0u8; 32];
        let engine = base64::engine::general_purpose::STANDARD;
        let secret_vec = engine.decode(secret_str).unwrap();
        for (i, b) in secret_vec.iter().enumerate() {
            secret[i] = *b;
        }

        let mut peer = [0u8; 32];
        let peer_vec = engine.decode(peer_str).unwrap();
        for (i, b) in peer_vec.iter().enumerate() {
            peer[i] = *b;
        }
        let self_key = x25519::StaticSecret::from(secret);
        let peer = x25519::PublicKey::from(peer);
        let test = WgTunDevice::new(
            self_key,
            peer,
        ).unwrap();
        let ip = IpCidr::new(IpAddress::v4(123, 123, 123, 3), 24);
        let mut stream = TcpStream::new(test, ip);

        let endpoint = smoltcp::wire::IpEndpoint::new(smoltcp::wire::IpAddress::v4(123, 123, 123, 2), 80);
        stream.connect(endpoint, 1234).unwrap();

        let stream = Arc::new(Mutex::new(stream));
        let stream2 = stream.clone();
        let window = web_sys::window().unwrap();
        let closure = Closure::<dyn FnMut(_)>::new(move |_: MessageEvent| {
            console_log!("interval elapsed");
            static mut SENT: bool = false;
            static mut START: Option<wasm_timer::Instant> = None;
            let mut stream = stream2.lock().unwrap();
            stream.poll();
            if stream.can_send() && !unsafe { SENT } {
                let request = request::Builder::new()
                    .method("GET")
                    .uri("/")
                    .body(())
                    .unwrap();

                let request = RequestWrap::new_get(request);
                let request = request.build_get();
                console_log!("sending {:?}", std::str::from_utf8(&request).unwrap());
                let len = stream.write(&request).unwrap();
                stream.flush().unwrap();
                unsafe { SENT = true };
                unsafe { START = Some(wasm_timer::Instant::now()) };
                console_log!("sent {} bytes", len);
            }

            static mut BYTES: usize = 0;
            static mut RECEIVED: bool = false;
            if stream.can_recv() {
                let mut buf = [0u8; 2048];
                let len = stream.read(&mut buf).unwrap();
                unsafe { BYTES += len };
                unsafe { RECEIVED = false };
                console_log!("received {:?} bytes", len);
            } else if !stream.can_recv() && unsafe { SENT } && !unsafe { RECEIVED } {
                unsafe { RECEIVED = true };
                let elapsed = unsafe { START.unwrap().elapsed() };
                console_log!("Took : {}ms to load {} bytes.", elapsed.as_millis(), unsafe { BYTES });
            }
        });
        window.set_interval_with_callback_and_timeout_and_arguments_0(
            closure.as_ref().unchecked_ref(),
            0,
        ).unwrap();
        closure.forget();

        Self {
            stream
        }
    }
}
