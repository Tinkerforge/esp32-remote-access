
use std::{sync::{Arc, Mutex}, task::Poll, io::Read};

use futures::{Future, FutureExt};
use http_body_util::{Empty, BodyExt};
use hyper::body::Bytes;
use smoltcp::wire::{IpAddress, IpCidr};
use wasm_bindgen::prelude::*;
use web_sys::{MessageEvent, window};
use crate::{stream::TcpStream, wg_device::WgTunDevice, hyper_stream::HyperStream};
use base64::Engine;
use boringtun::x25519;
use flate2::read::GzDecoder;
// use tokio::io::{AsyncWriteExt as _, self};

use crate::console_log;

#[wasm_bindgen]
pub struct WgClient {
    stream: Arc<Mutex<TcpStream<'static, WgTunDevice>>>,
}

#[derive(PartialEq)]
enum RequestState {
    Started,
    Connected,
    HandshakeDone,
    RequestSent,
    StreamingBody,
    Done,
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
        // stream.connect(endpoint, 1234).unwrap();

        // wasm_thread::spawn(move || {
        //     console_log!("Hello from a web worker!");
        // });

        let stream = Arc::new(Mutex::new(stream));
        let stream2 = stream.clone();
        let window = web_sys::window().unwrap();
        let closure = Closure::<dyn FnMut(_)>::new(move |_: MessageEvent| {
            static mut SENT: bool = false;
            static mut START: Option<wasm_timer::Instant> = None;
            let mut stream = stream2.lock().unwrap();
            stream.poll();
            // if stream.can_send() && !unsafe { SENT } {
            //     let request = request::Builder::new()
            //         .method("GET")
            //         .uri("/")
            //         .body(())
            //         .unwrap();

            //     let request = RequestWrap::new_get(request);
            //     let request = request.build_get();
            //     console_log!("sending {:?}", std::str::from_utf8(&request).unwrap());
            //     let len = stream.write(&request).unwrap();
            //     stream.flush().unwrap();
            //     unsafe { SENT = true };
            //     unsafe { START = Some(wasm_timer::Instant::now()) };
            //     console_log!("sent {} bytes", len);
            // }

            // static mut BYTES: usize = 0;
            // static mut RECEIVED: bool = false;
            // if stream.can_recv() {
            //     let mut buf = [0u8; 2048];
            //     let len = stream.read(&mut buf).unwrap();
            //     unsafe { BYTES += len };
            //     unsafe { RECEIVED = false };
            // } else if !stream.can_recv() && unsafe { SENT } && !unsafe { RECEIVED } {
            //     unsafe { RECEIVED = true };
            //     let elapsed = unsafe { START.unwrap().elapsed() };
            //     console_log!("Took : {}ms to load {} bytes.", elapsed.as_millis(), unsafe { BYTES });
            // }
        });
        window.set_interval_with_callback_and_timeout_and_arguments_0(
            closure.as_ref().unchecked_ref(),
            0,
        ).unwrap();
        closure.forget();

        Self {
            stream,
        }
    }

    #[wasm_bindgen]
    pub async fn get(&self, url: &str) -> String {
        console_error_panic_hook::set_once();
        console_log!("get: {:?}", url);
        {
            let endpoint = smoltcp::wire::IpEndpoint::new(smoltcp::wire::IpAddress::v4(123, 123, 123, 2), 80);
            self.stream.lock().unwrap().connect(endpoint, 80).unwrap();
        }

        let id = js_sys::Math::random();

        let state = Arc::new(Mutex::new(RequestState::Started));
        let state_cpy = state.clone();
        let stream_cpy = self.stream.clone();

        let sender = Arc::new(Mutex::new(None));
        let sender_cpy = sender.clone();
        let conn = Arc::new(Mutex::new(None));
        let conn_cpy = conn.clone();
        let request = Arc::new(Mutex::new(None));
        let resp = Arc::new(Mutex::new(None));

        let result = Arc::new(Mutex::new(vec!{0u8; 0}));

        let url = Arc::new(url.to_string());

        let connect = Closure::<dyn FnMut(_)>::new(move |_: MessageEvent| {
            let stream_cpy = stream_cpy.clone();
            let mut state = state_cpy.lock().unwrap();

            match *state {
                RequestState::Started => {
                    let mut stream = stream_cpy.lock().unwrap();
                    if stream.can_send() {
                        console_log!("can send");
                        *state = RequestState::Connected;
                    }
                },
                RequestState::Connected => {
                    let is_none;
                    {
                        let sender = sender.lock().unwrap();
                        is_none = sender.is_none();
                    }

                    if is_none {
                        let sender_cpy = sender_cpy.clone();
                        let conn_cpy = conn_cpy.clone();
                        let stream_cpy = stream_cpy.clone();
                        let state_cpy = state_cpy.clone();
                        wasm_bindgen_futures::spawn_local(async move {
                            console_log!("handshake");
                            let stream = HyperStream::new(stream_cpy.clone());
                            let (sender, conn) = hyper::client::conn::http1::handshake(stream).await.unwrap();
                            console_log!("handshake done");
                            *sender_cpy.lock().unwrap() = Some(sender);
                            *conn_cpy.lock().unwrap() = Some(Box::pin(conn));
                            *state_cpy.lock().unwrap() = RequestState::HandshakeDone;
                        });
                    }
                },
                RequestState::HandshakeDone => {
                    let req = http::Request::builder()
                    .method("GET")
                    .uri(url.as_ref())
                    .body(Empty::<Bytes>::new())
                    .unwrap();
                    let mut sender = sender.lock().unwrap();
                    let sender = sender.as_mut().unwrap();
                    let mut request = request.lock().unwrap();
                    *request = Some(Box::pin(sender.send_request(req)));
                    *state = RequestState::RequestSent;
                    console_log!("request sent");
                },
                RequestState::RequestSent => {
                    console_log!("request poll");
                    let waker = futures::task::noop_waker();
                    let mut cx = std::task::Context::from_waker(&waker);
                    let mut request = request.lock().unwrap();
                    let request = request.as_mut().unwrap();
                    match request.as_mut().poll(&mut cx) {
                        Poll::Ready(Ok(response)) => {
                            console_log!("resp: {:?}", response);
                            *resp.lock().unwrap() = Some(Box::pin(response));
                            *state = RequestState::StreamingBody;
                        },
                        Poll::Ready(Err(e)) => panic!("error: {}", e),
                        Poll::Pending => (),
                    };
                    let mut conn = conn.lock().unwrap();
                    let conn = conn.as_mut().unwrap();
                    match conn.as_mut().poll(&mut cx) {
                        Poll::Ready(Ok(_)) => (),
                        Poll::Ready(Err(e)) => panic!("error: {}", e),
                        Poll::Pending => (),
                    };
                },
                RequestState::StreamingBody => {
                    let waker = futures::task::noop_waker();
                    let mut cx = std::task::Context::from_waker(&waker);
                    let mut resp = resp.lock().unwrap();
                    let resp = resp.as_mut().unwrap();
                    match resp.frame().poll_unpin(&mut cx) {
                        Poll::Ready(Some(Ok(frame))) => {
                            if let Some(chunk) = frame.data_ref() {
                                let mut result = result.lock().unwrap();
                                result.extend_from_slice(chunk);
                            }
                        },
                        Poll::Ready(Some(Err(e))) => panic!("error: {}", e),
                        Poll::Ready(None) => {
                            console_log!("done");
                            *state = RequestState::Done;
                            let result = result.lock().unwrap();
                            let mut gz = GzDecoder::new(&result[..]);
                            let mut s = String::new();
                            gz.read_to_string(&mut s).unwrap();
                            // console_log!("result: {:?}", result);
                            let arr = js_sys::Uint8Array::from(s.as_bytes());
                            let mut init = web_sys::CustomEventInit::new();
                            init.detail(&arr);
                            let event = web_sys::CustomEvent::new_with_event_init_dict(format!("get_{}", id).as_str(), &init).unwrap();
                            window().unwrap().dispatch_event(&event).unwrap();
                        },
                        Poll::Pending => (),
                    };
                    let mut conn = conn.lock().unwrap();
                    let conn = conn.as_mut().unwrap();
                    match conn.as_mut().poll(&mut cx) {
                        Poll::Ready(Ok(_)) => (),
                        Poll::Ready(Err(e)) => panic!("error: {}", e),
                        Poll::Pending => (),
                    };
                },
                _ => {},
            }

        });
        // let interval = IntervalHandle::new(connect, 0);
        // let connect = Rc::new(interval);

        window().unwrap().set_interval_with_callback_and_timeout_and_arguments_0(
            connect.as_ref().unchecked_ref(),
            0,
        ).unwrap();
        connect.forget();

        format!("get_{}", id)
    }
}
