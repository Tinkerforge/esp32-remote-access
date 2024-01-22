
use std::{task::Poll, io::Read, collections::VecDeque, rc::Rc, cell::RefCell, ops::Deref};

use futures::{Future, FutureExt};
use gloo_file::{File, ObjectUrl};
use http_body_util::BodyExt;
use hyper::body::Bytes;
use js_sys::Promise;
use pcap_file::pcapng::PcapNgWriter;
use smoltcp::wire::{IpAddress, IpCidr};
use wasm_bindgen::prelude::*;
use web_sys::{MessageEvent, window, CustomEvent};
use crate::{stream::TcpStream, wg_device::WgTunDevice, hyper_stream::HyperStream, interval_handle::IntervalHandle, interface::Interface};
use base64::Engine;
use boringtun::x25519;
use flate2::read::GzDecoder;

use crate::console_log;
#[wasm_bindgen]
struct Client(Rc::<WgClient>, Rc<RefCell<VecDeque<Closure<dyn FnMut(CustomEvent)>>>>);

#[wasm_bindgen]
impl Client {
    #[wasm_bindgen(constructor)]
    pub fn new(secret_str: &str, peer_str: &str, url: &str) -> Self {
        Self(Rc::new(WgClient::new(secret_str, peer_str, url)), Rc::new(RefCell::new(VecDeque::new())))
    }

    #[wasm_bindgen]
    pub fn fetch(&self, uri: String, method: String, body: Option<Vec<u8>>) -> Promise {
        let cpy = self.0.clone();
        let id = cpy.fetch(uri, method, body, None);
        Promise::new(&mut move |resolve, _| {
            let queue = self.1.clone();
            let closure = Closure::<dyn FnMut(_)>::new(move |event: CustomEvent| {
                let response = event.detail();
                let _ = queue.borrow_mut().pop_front();
                resolve.call1(&JsValue::NULL, &response).unwrap();
            });
            window().unwrap().add_event_listener_with_callback(id.as_str(), closure.as_ref().unchecked_ref()).unwrap();
            self.1.borrow_mut().push_back(closure);
        })
    }

    // leaks memory, for debugging only!!!
    #[wasm_bindgen]
    pub fn download_pcap_log(&self) {
        let cpy = self.0.clone();
        cpy.download_pcap_log();
    }
}

struct WgClient {
    stream: Rc<RefCell<TcpStream<'static, WgTunDevice>>>,
    current_request: Rc<RefCell<Option<IntervalHandle<MessageEvent>>>>,
    request_queue: Rc<RefCell<VecDeque<Request>>>,
    pcap: Rc<RefCell<PcapNgWriter<Vec<u8>>>>,
}

enum RequestState {
    Started,
    Connected,
    HandshakeDone,
    RequestSent,
    StreamingBody,
    Done,
}

impl WgClient {
    fn new(secret_str: &str, peer_str: &str, url: &str) -> Self {
        console_error_panic_hook::set_once();

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
        let device = WgTunDevice::new(
            self_key,
            peer,
            url
        ).unwrap();

        let pcap = device.get_pcap();

        let ip = IpCidr::new(IpAddress::v4(123, 123, 123, 3), 24);
        let iface = Rc::new(RefCell::new(Interface::new(device, ip)));
        let stream = TcpStream::new(iface);

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
            request_queue: Rc::new(RefCell::new(VecDeque::new())),
            pcap,
        }
    }

    fn fetch(self: Rc<Self>, uri: String, method: String, body: Option<Vec<u8>>, in_id: Option<f64>) -> String {
        let id;
        if let Some(in_id) = in_id {
            id = in_id;
        } else {
            id = js_sys::Math::random();
        }
        if self.current_request.borrow_mut().is_some() {
            let mut request_queue = self.request_queue.borrow_mut();
            request_queue.push_back(Request {
                id,
                uri,
                method,
                body,
            });
            return format!("get_{}", id);
        }
        console_log!("get: {:?}", uri);
        {
            let port = js_sys::Math::random() * 1000.0;
            let port = port as u16;
            let endpoint = smoltcp::wire::IpEndpoint::new(smoltcp::wire::IpAddress::v4(123, 123, 123, 2), 80);
            console_log!("before");
            self.stream.borrow_mut().connect(endpoint, port).unwrap();
            console_log!("after");
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
        let method = Rc::new(method);
        let body = Rc::new(body);

        let request_queue = self.request_queue.clone();
        let current_request = self.current_request.clone();
        let self_cpy = self.clone();

        let req = Closure::<dyn FnMut(_)>::new(move |_: MessageEvent| {
            let stream_cpy = stream_cpy.clone();
            let mut state = state_cpy.borrow_mut();
            let self_cpy = self_cpy.clone();

            match *state {
                RequestState::Started => {
                    let stream = stream_cpy.borrow_mut();
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
                    let method = match method.as_ref().as_str() {
                        "GET" => hyper::Method::GET,
                        "PUT" => hyper::Method::PUT,
                        // FIXME: throw exception instead of panic
                        _ => panic!("unknown method: {}", method),
                    };
                    let body = match body.deref() {
                        Some(body) => &body[..],
                        None => &[][..],
                    };
                    let req = http::Request::builder()
                    .method(method)
                    .header("Content-Type", "application/json; charset=utf-8")
                    .uri(uri.as_ref())
                    .body(Box::new(Body::new(Bytes::copy_from_slice(body))))
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
                            *resp.borrow_mut() = Some(Box::pin(response));
                            *state = RequestState::StreamingBody;
                        },
                        // FIXME: throw exception instead of panic
                        Poll::Ready(Err(e)) => panic!("error: {}", e),
                        Poll::Pending => (),
                    };
                    let mut conn = conn.borrow_mut();
                    let conn = conn.as_mut().unwrap();
                    match conn.as_mut().poll(&mut cx) {
                        Poll::Ready(Ok(_)) => (),
                        // FIXME: throw exception instead of panic
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
                        // FIXME: throw exception instead of panic
                        Poll::Ready(Some(Err(e))) => panic!("error: {}", e),
                        Poll::Ready(None) => {
                            console_log!("done");
                            *state = RequestState::Done;
                            let result = result.borrow_mut();
                            let mut body = if let Some(encoding) = resp.headers().get("Content-Encoding") {
                                if encoding == "gzip" {
                                    let mut gz = GzDecoder::new(&result[..]);
                                    let mut s = String::new();
                                    gz.read_to_string(&mut s).unwrap();
                                    s.as_bytes().to_vec()
                                } else {
                                    // FIXME: handle other encodings and throw exception instead of panic
                                    panic!("unknown encoding: {}", encoding.to_str().unwrap());
                                }
                            } else {
                                result.clone()
                            };

                            let headers = web_sys::Headers::new().unwrap();
                            for (key, value) in resp.headers().iter() {
                                let value = match value.to_str() {
                                    Ok(value) => value,
                                    Err(_) => continue,
                                };
                                headers.set(key.as_str(), value).unwrap();
                            }
                            let mut response_init = web_sys::ResponseInit::new();
                            response_init.status(resp.status().as_u16());
                            response_init.headers(&headers);
                            response_init.status_text(resp.status().canonical_reason().unwrap_or(""));
                            let response = web_sys::Response::new_with_opt_u8_array_and_init(Some(&mut body[..]), &response_init).unwrap();
                            let mut init = web_sys::CustomEventInit::new();
                            init.detail(&response.into());
                            let event = web_sys::CustomEvent::new_with_event_init_dict(format!("get_{}", id).as_str(), &init).unwrap();
                            window().unwrap().dispatch_event(&event).unwrap();
                        },
                        Poll::Pending => (),
                    };
                    let mut conn = conn.borrow_mut();
                    let conn = conn.as_mut().unwrap();
                    match conn.as_mut().poll(&mut cx) {
                        Poll::Ready(Ok(_)) => (),
                        // FIXME: throw exception instead of panic
                        Poll::Ready(Err(e)) => panic!("error: {}", e),
                        Poll::Pending => (),
                    };
                },
                _ => {
                    let stream_cpy = stream_cpy.clone();
                    let mut stream = stream_cpy.borrow_mut();
                    stream.close();
                    stream.poll();
                    if stream.is_open() {
                        return;
                    }
                    *current_request.borrow_mut() = None;
                    let mut request_queue = request_queue.borrow_mut();
                    if let Some(next) = request_queue.pop_front() {
                        wasm_bindgen_futures::spawn_local(async move {
                            self_cpy.fetch(next.uri, next.method, next.body, Some(next.id));
                        })
                    }
                },
            }

        });
        let interval = IntervalHandle::new(req, 0);
        *self.current_request.borrow_mut() = Some(interval);

        format!("get_{}", id)
    }


    // leaks memory, for debugging only!!!
    pub fn download_pcap_log(&self) {
        let pcap = self.pcap.borrow_mut();
        let content = pcap.get_ref().to_owned();
        let file = File::new("out.pcap", &content[..]);
        let file = ObjectUrl::from(file);

        let window = web_sys::window().unwrap();
        let document = window.document().unwrap();
        let element = document.create_element("a").unwrap();
        element.set_attribute("download", "out.pcap").unwrap();
        element.set_attribute("href", &file.to_string()).unwrap();
        element.set_attribute("target", "_blank").unwrap();
        let element = wasm_bindgen::JsValue::from(element);
        let element = web_sys::HtmlElement::from(element);
        element.click();
    }
}

struct Request {
    pub id: f64,
    pub uri: String,
    pub method: String,
    pub body: Option<Vec<u8>>,
}

struct Body(Bytes);

impl Body {
    fn new(bytes: Bytes) -> Self {
        Self(bytes)
    }
}

impl hyper::body::Body for Body {
    type Data = Bytes;
    type Error = DummyErr;

    fn poll_frame(
            self: std::pin::Pin<&mut Self>,
            _: &mut std::task::Context<'_>,
        ) -> Poll<Option<Result<hyper::body::Frame<Self::Data>, Self::Error>>> {
            Poll::Ready(Some(Ok(hyper::body::Frame::data(self.0.clone()))))
    }

    fn size_hint(&self) -> hyper::body::SizeHint {
        hyper::body::SizeHint::with_exact(self.0.len() as u64)
    }
}

/**
 * This is a dummy error type that is never returned. It is needed because the Body trait needs an Error type.
 */
#[derive(Debug)]
struct DummyErr;

impl std::error::Error for DummyErr {}

impl std::fmt::Display for DummyErr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "how the f did this happen!?")
    }
}
