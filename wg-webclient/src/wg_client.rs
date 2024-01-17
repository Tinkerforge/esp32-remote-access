
use std::{task::Poll, io::Read, collections::VecDeque, rc::Rc, cell::RefCell};

use futures::{Future, FutureExt};
use http_body_util::{Empty, BodyExt};
use hyper::body::Bytes;
use smoltcp::wire::{IpAddress, IpCidr};
use wasm_bindgen::prelude::*;
use web_sys::{MessageEvent, window};
use crate::{stream::TcpStream, wg_device::WgTunDevice, hyper_stream::HyperStream, interval_handle::IntervalHandle};
use base64::Engine;
use boringtun::x25519;
use flate2::read::GzDecoder;
// use tokio::io::{AsyncWriteExt as _, self};

use crate::console_log;

#[wasm_bindgen]
pub struct WgClient {
    stream: Rc<RefCell<TcpStream<'static, WgTunDevice>>>,
    current_request: Rc<RefCell<Option<IntervalHandle<MessageEvent>>>>,
    request_queue: Rc<RefCell<VecDeque<Request>>>
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
        let stream = TcpStream::new(test, ip);

        let stream = Rc::new(RefCell::new(stream));
        let stream2 = stream.clone();
        let window = web_sys::window().unwrap();
        let closure = Closure::<dyn FnMut(_)>::new(move |_: MessageEvent| {
            let mut stream = stream2.borrow_mut();
            stream.poll();
        });
        window.set_interval_with_callback_and_timeout_and_arguments_0(
            closure.as_ref().unchecked_ref(),
            0,
        ).unwrap();
        closure.forget();

        Self {
            stream,
            current_request: Rc::new(RefCell::new(None)),
            request_queue: Rc::new(RefCell::new(VecDeque::new()))
        }
    }

    #[wasm_bindgen]
    pub fn fetch(&self, uri: String, method: String, body: Option<Vec<u8>>) -> String {
        console_error_panic_hook::set_once();
        let id = js_sys::Math::random();
        if self.current_request.borrow_mut().is_some() {
            let mut request_queue = self.request_queue.borrow_mut();
            request_queue.push_back(Request {
                uri,
                method,
                body,
            });
            return format!("get_{}", id);
        }
        console_log!("get: {:?}", uri);
        {
            let endpoint = smoltcp::wire::IpEndpoint::new(smoltcp::wire::IpAddress::v4(123, 123, 123, 2), 80);
            self.stream.borrow_mut().connect(endpoint, 80).unwrap();
        }

        let state = Rc::new(RefCell::new(RequestState::Started));
        let state_cpy = state.clone();
        let stream_cpy = self.stream.clone();

        let sender = Rc::new(RefCell::new(None));
        let sender_cpy = sender.clone();
        let conn = Rc::new(RefCell::new(None));
        let conn_cpy = conn.clone();
        let request = Rc::new(RefCell::new(None));
        let resp = Rc::new(RefCell::new(None));

        let result = Rc::new(RefCell::new(vec!{0u8; 0}));

        let uri = Rc::new(uri);

        let connect = Closure::<dyn FnMut(_)>::new(move |_: MessageEvent| {
            let stream_cpy = stream_cpy.clone();
            let mut state = state_cpy.borrow_mut();

            match *state {
                RequestState::Started => {
                    let mut stream = stream_cpy.borrow_mut();
                    if stream.can_send() {
                        console_log!("can send");
                        *state = RequestState::Connected;
                    }
                },
                RequestState::Connected => {
                    let is_none;
                    {
                        let sender = sender.borrow_mut();
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
                            *sender_cpy.borrow_mut() = Some(sender);
                            *conn_cpy.borrow_mut() = Some(Box::pin(conn));
                            *state_cpy.borrow_mut() = RequestState::HandshakeDone;
                        });
                    }
                },
                RequestState::HandshakeDone => {
                    let req = http::Request::builder()
                    .method("GET")
                    .uri(uri.as_ref())
                    .body(Empty::<Bytes>::new())
                    .unwrap();
                    let mut sender = sender.borrow_mut();
                    let sender = sender.as_mut().unwrap();
                    let mut request = request.borrow_mut();
                    *request = Some(Box::pin(sender.send_request(req)));
                    *state = RequestState::RequestSent;
                    console_log!("request sent");
                },
                RequestState::RequestSent => {
                    console_log!("request poll");
                    let waker = futures::task::noop_waker();
                    let mut cx = std::task::Context::from_waker(&waker);
                    let mut request = request.borrow_mut();
                    let request = request.as_mut().unwrap();
                    match request.as_mut().poll(&mut cx) {
                        Poll::Ready(Ok(response)) => {
                            console_log!("resp: {:?}", response);
                            *resp.borrow_mut() = Some(Box::pin(response));
                            *state = RequestState::StreamingBody;
                        },
                        Poll::Ready(Err(e)) => panic!("error: {}", e),
                        Poll::Pending => (),
                    };
                    let mut conn = conn.borrow_mut();
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
                    let mut resp = resp.borrow_mut();
                    let resp = resp.as_mut().unwrap();
                    match resp.frame().poll_unpin(&mut cx) {
                        Poll::Ready(Some(Ok(frame))) => {
                            if let Some(chunk) = frame.data_ref() {
                                let mut result = result.borrow_mut();
                                result.extend_from_slice(chunk);
                            }
                        },
                        Poll::Ready(Some(Err(e))) => panic!("error: {}", e),
                        Poll::Ready(None) => {
                            console_log!("done");
                            *state = RequestState::Done;
                            let result = result.borrow_mut();
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
                    let mut conn = conn.borrow_mut();
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

struct Request {
    pub uri: String,
    pub method: String,
    pub body: Option<Vec<u8>>,
}
