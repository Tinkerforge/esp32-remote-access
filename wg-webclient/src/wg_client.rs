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

use std::{cell::RefCell, collections::VecDeque, io::Read, rc::Rc, task::Poll};

use crate::{
    hyper_stream::HyperStream, interface::Interface, interval_handle::IntervalHandle,
    stream::TcpStream, websocket::Websocket, wg_device::WgTunDevice,
};
use base64::Engine;
use boringtun::x25519;
use flate2::read::GzDecoder;
use futures::{Future, FutureExt};
use gloo_file::{File, ObjectUrl};
use http_body_util::BodyExt;
use hyper::body::Bytes;
use js_sys::Promise;
use pcap_file::pcapng::PcapNgWriter;
use smoltcp::wire::IpCidr;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;
use web_sys::{CustomEvent, MessageEvent, Response};

/**
 * The exported client struct. It Wraps the actual Client and a Queue to keep the needed
 * callbacks alive.
 * Most function calls are simply passed to the wrapped object.
 */
#[wasm_bindgen]
pub struct Client(
    WgClient,
    Rc<RefCell<VecDeque<Closure<dyn FnMut(CustomEvent)>>>>,
);

#[wasm_bindgen]
impl Client {
    /**
     * Creates a new Client struct by also creating the wrapped objects.
     */
    #[wasm_bindgen(constructor)]
    pub fn new(
        secret_str: &str,
        peer_str: &str,
        psk: &str,
        url: &str,
        internal_ip: &str,
        internap_peer_ip: &str,
        port: u16,
        disconnect_cb: js_sys::Function,
    ) -> Self {
        console_log::init_with_level(log::Level::Debug).unwrap();
        Self(
            WgClient::new(secret_str, peer_str, psk, url, internal_ip, internap_peer_ip, port, disconnect_cb),
            Rc::new(RefCell::new(VecDeque::new())),
        )
    }

    fn create_await_promise(&self, req_id: String) -> Promise {
        let queue = self.1.clone();
        Promise::new(&mut move |resolve, _| {
            let queue_cpy = queue.clone();
            let closure = Closure::<dyn FnMut(_)>::new(move |event: CustomEvent| {
                let response = event.detail();
                let _ = queue_cpy.borrow_mut().pop_front();
                resolve.call1(&JsValue::NULL, &response).unwrap();
            });

            // add event listener should not fail
            let global = js_sys::global();
            let global = web_sys::WorkerGlobalScope::from(JsValue::from(global));
            let mut options = web_sys::AddEventListenerOptions::new();
            options.once(true);
            global
                .add_event_listener_with_callback_and_add_event_listener_options(
                    req_id.as_str(),
                    closure.as_ref().unchecked_ref(),
                    &options,
                )
                .unwrap();
            queue.borrow_mut().push_back(closure);
        })
    }

    /**
     * Makes a http request to the provided url and return a Promise that resolves to a JS Response object.
     * Internally it calls the fetch function of the wrapped WgClient object and
     * registers an EventListener for the event returned by it.
     */
    #[wasm_bindgen]
    pub async fn fetch(&self, request: web_sys::Request, url: String, username: Option<String>, password: Option<String>) -> Response {
        let id = self.0.clone().fetch(request.clone().unwrap(), url.clone(), None, username.clone(), password.clone());
        let promise = self.create_await_promise(id);
        let result = wasm_bindgen_futures::JsFuture::from(promise).await.unwrap();
        let mut response = Response::from(result);

        if response.status() == 401 {
            let id = self.0.clone().fetch(request, url, None, username, password);
            let promise = self.create_await_promise(id);
            let result = wasm_bindgen_futures::JsFuture::from(promise).await.unwrap();
            response = Response::from(result);
        }

        response
    }

    #[wasm_bindgen]
    pub fn start_inner_ws(&self, cb: js_sys::Function) {
        self.0.clone().start_inner_ws(cb);
    }

    #[wasm_bindgen]
    pub fn disconnect_inner_ws(&self) {
        self.0.clone().disconnect_inner_ws();
    }

    // leaks memory, for debugging only!!!
    #[wasm_bindgen]
    pub fn download_pcap_log(&self) {
        let cpy = self.0.clone();
        cpy.download_pcap_log();
    }

    #[wasm_bindgen]
    pub fn get_pcap_log(&self) -> Vec<u8> {
        let cpy = self.0.clone();
        cpy.get_pcap_log()
    }
}

/**
 * The struct that acutally does all the work
 */
#[derive(Clone)]
struct WgClient {
    stream: Rc<RefCell<TcpStream<'static, WgTunDevice>>>,
    iface: Rc<RefCell<Interface<'static, WgTunDevice>>>,
    websocket: Rc<RefCell<Option<Websocket<WgTunDevice>>>>,
    current_request: Rc<RefCell<Option<IntervalHandle<MessageEvent>>>>,
    request_queue: Rc<RefCell<VecDeque<Request>>>,
    pcap: Rc<RefCell<PcapNgWriter<Vec<u8>>>>,
    internal_peer_ip: String,
    _polling_interval: Rc<IntervalHandle<MessageEvent>>,
    username: Rc<RefCell<String>>,
    password: Rc<RefCell<String>>,
    realm: Rc<RefCell<String>>,
    nonce: Rc<RefCell<String>>,
    opaque: Rc<RefCell<String>>,
    nc: Rc<RefCell<i64>>,
    port: u16,
}

enum RequestState {
    Begin,
    Started,
    Connected,
    HandshakeDone,
    SendingRequest,
    RequestSent,
    StreamingBody,
    Done,
}

impl WgClient {
    /**
     * Creates a new object.
     */
    fn new(
        secret_str: &str,
        peer_str: &str,
        psk: &str,
        url: &str,
        internal_ip: &str,
        internal_peer_ip: &str,
        port: u16,
        disconnet_cb: js_sys::Function,
    ) -> Self {
        console_error_panic_hook::set_once();

        let engine = base64::engine::general_purpose::STANDARD;

        // decoding secret and peer public key should fail very noticably. So either panic or throw exception
        let secret_vec = engine.decode(secret_str).unwrap();
        let secret: [u8; 32] = secret_vec.try_into().unwrap();

        let peer_vec = engine.decode(peer_str).unwrap();
        let peer: [u8; 32] = peer_vec.try_into().unwrap();

        let self_key = x25519::StaticSecret::from(secret);
        let peer = x25519::PublicKey::from(peer);
        let psk = engine.decode(psk).unwrap();
        let psk: [u8; 32] = psk.try_into().unwrap();

        // same as above
        let device = WgTunDevice::new(self_key, peer, psk, url, disconnet_cb).unwrap();

        let pcap = device.get_pcap();

        let ip = &internal_ip[0..internal_ip.len() - 3];
        let ip = IpCidr::new(ip.parse().unwrap(), 24);
        let iface = Rc::new(RefCell::new(Interface::new(device, ip)));
        let iface_cpy = Rc::downgrade(&iface);

        let stream = TcpStream::new(iface.clone());

        let stream = Rc::new(RefCell::new(stream));

        let closure = Closure::<dyn FnMut(_)>::new(move |_: MessageEvent| {
            iface_cpy.upgrade().unwrap().borrow_mut().poll();
        });
        let polling_interval = Rc::new(IntervalHandle::new(closure, 0));

        Self {
            stream,
            iface,
            websocket: Rc::new(RefCell::new(None)),
            current_request: Rc::new(RefCell::new(None)),
            request_queue: Rc::new(RefCell::new(VecDeque::new())),
            pcap,
            internal_peer_ip: (&internal_peer_ip[0..internal_peer_ip.len() - 3]).to_string(),
            _polling_interval: polling_interval,
            username: Rc::new(RefCell::new(String::new())),
            password: Rc::new(RefCell::new(String::new())),
            realm: Rc::new(RefCell::new(String::new())),
            nonce: Rc::new(RefCell::new(String::new())),
            opaque: Rc::new(RefCell::new(String::new())),
            nc: Rc::new(RefCell::new(1)),
            port
        }
    }

    /**
     * Creates a new Websocket object and connection that gets stored internally.
     */
    fn start_inner_ws(&mut self, cb: js_sys::Function) {
        let mut stream = TcpStream::new(self.iface.clone());
        let port = js_sys::Math::random() * 1000.0;
        let port = port as u16;
        let endpoint = smoltcp::wire::IpEndpoint::new(self.internal_peer_ip.parse().unwrap(), self.port);
        if let Err(err) = stream.connect(endpoint, port) {
            log::error!("Error when connecting websocket: {}", err.to_string());
        }
        let auth_header = if self.nonce.borrow().len() != 0 {
            Some(self.build_authentication_header("GET", "/ws"))
        } else {
            None
        };
        let mut websocket = Websocket::connect(stream, auth_header).unwrap();
        websocket.on_message(cb);
        *self.websocket.borrow_mut() = Some(websocket);
    }

    fn build_authentication_header(&self, method: &str, uri: &str) ->  String {
        let ha1 = md5::compute(format!("{}:{}:{}", self.username.borrow(), self.realm.borrow(), self.password.borrow()));
        let ha2 = md5::compute(format!("{}:{}", method, uri));

        let this: JsValue = js_sys::global().try_into().unwrap();
        let this = web_sys::WorkerGlobalScope::from(this);
        let crypto = this.crypto().unwrap();

        let cnonce = md5::compute(crypto.random_uuid());
        let response = md5::compute(format!("{:x}:{}:{:08x}:{:x}:auth:{:x}", ha1, self.nonce.borrow(), *self.nc.borrow(), cnonce, ha2));

        let result = format!("Digest username=\"{}\",realm=\"{}\",nonce=\"{}\",uri=\"{}\",qop=auth,nc={:08x},cnonce=\"{:x}\",response=\"{:x}\",opaque=\"{}\"",
            self.username.borrow(),
            self.realm.borrow(),
            self.nonce.borrow(),
            uri,
            *self.nc.borrow(),
            cnonce,
            response,
            self.opaque.borrow());

        *self.nc.borrow_mut() += 1;

        result
    }

    /**
     The function that actually does the http requests.
     Since we must not block the thread while waiting for the response because it would also block
     the underlaying network stack it starts a task with setTimeout that polls the so far received
     response, fires a custom event when finished and either returns or proceeds with the next
     request.
    */
    fn fetch(&self, js_request: web_sys::Request, url: String, in_id: Option<f64>, username: Option<String>, password: Option<String>) -> String {
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
                js_request,
                url,
                username,
                password,
            });
            return format!("get_{}", id);
        }

        if let Some(username) = username {
            *self.username.borrow_mut() = urlencoding::decode(&username).unwrap().into_owned();
        }
        if let Some(password) = password {
            *self.password.borrow_mut() = urlencoding::decode(&password).unwrap().into_owned();
        }

        let internal_peer_ip = self.internal_peer_ip.parse().unwrap();
        let state = Rc::new(RefCell::new(RequestState::Begin));
        let state_cpy = state.clone();
        let stream_cpy = Rc::downgrade(&self.stream);

        let sender = Rc::new(RefCell::new(None));
        let sender_cpy = Rc::downgrade(&sender);
        let conn = Rc::new(RefCell::new(None));
        let conn_cpy = Rc::downgrade(&conn);
        let request = Rc::new(RefCell::new(None));
        let resp = Rc::new(RefCell::new(None));

        let result = Rc::new(RefCell::new(vec![0u8; 0]));

        let url = Rc::new(url);
        let js_request = Rc::new(js_request);

        let request_queue = Rc::downgrade(&self.request_queue);
        let current_request = Rc::downgrade(&self.current_request);
        let self_cpy = self.clone();

        let port = self.port;

        let req = Closure::<dyn FnMut(_)>::new(move |_: MessageEvent| {
            if !stream_cpy.upgrade().unwrap().borrow().is_up() {
                return;
            }
            let stream_cpy = stream_cpy.clone();
            let mut state = state_cpy.borrow_mut();
            let self_cpy = self_cpy.clone();

            match *state {
                RequestState::Begin => {
                    let out_port = js_sys::Math::random() * 1000.0;
                    let out_port = out_port as u16;
                    let endpoint = smoltcp::wire::IpEndpoint::new(internal_peer_ip, port);

                    // FIXME: throw exception instead of panic
                    if let Err(err) = stream_cpy.upgrade().unwrap().borrow_mut().connect(endpoint, out_port) {
                        log::error!("Error connecting to endpoint {}: {}", endpoint, err.to_string());
                        return;
                    }
                    *state = RequestState::Started;
                }
                RequestState::Started => {
                    let stream_cpy = stream_cpy.upgrade().unwrap();
                    let stream = stream_cpy.borrow_mut();
                    if stream.can_send() {
                        *state = RequestState::Connected;
                    }
                }
                RequestState::Connected => {
                    let is_none = sender.borrow_mut().is_none();

                    if is_none {
                        let sender_cpy = sender_cpy.clone();
                        let conn_cpy = conn_cpy.clone();
                        let stream_cpy = stream_cpy.clone();
                        let state_cpy = state_cpy.clone();
                        wasm_bindgen_futures::spawn_local(async move {
                            let stream = HyperStream::new(stream_cpy.upgrade().unwrap().clone());

                            // FIXME: throw exception instead of panic
                            let (sender, conn) =
                                hyper::client::conn::http1::handshake(stream).await.unwrap();
                            *sender_cpy.upgrade().unwrap().borrow_mut() = Some(sender);
                            *conn_cpy.upgrade().unwrap().borrow_mut() = Some(Box::pin(conn));
                            *state_cpy.borrow_mut() = RequestState::HandshakeDone;
                        });
                    }
                }
                RequestState::HandshakeDone => {
                    let js_request_cpy = js_request.clone();
                    let sender_cpy = sender.clone();
                    let request_cpy = request.clone();
                    let state_cpy = state_cpy.clone();
                    let url_cpy = url.clone();
                    wasm_bindgen_futures::spawn_local(async move {
                        let body = JsFuture::from(js_request_cpy.array_buffer().unwrap())
                            .await
                            .unwrap();
                        let body = js_sys::Uint8Array::new(&body).to_vec();
                        let method_str = js_request_cpy.method();
                        let method = match method_str.as_str() {
                            "GET" => hyper::Method::GET,
                            "PUT" => hyper::Method::PUT,
                            "POST" => hyper::Method::POST,
                            // FIXME: throw exception instead of panic
                            _ => panic!("unknown method: {}", js_request_cpy.method()),
                        };

                        let mut req = http::Request::builder().method(method);

                        let headers = js_request_cpy.headers();
                        for key in js_sys::Object::keys(&headers).iter() {
                            let key = key.as_string().unwrap();
                            let value = if let Ok(Some(value)) = headers.get(&key) {
                                value
                            } else {
                                continue;
                            };
                            req = req.header(key, value);
                        }


                        if self_cpy.nonce.borrow().len() != 0 {
                            let auth_header = self_cpy.build_authentication_header(&method_str, &url_cpy);
                            req = req.header("Authorization", auth_header);
                        }

                        let req = req
                            .header("Content-Type", "application/json; charset=utf-8")
                            .uri(url_cpy.as_str())
                            .body(Box::new(Body::new(Bytes::copy_from_slice(&body))))
                            .unwrap();
                        let mut sender = sender_cpy.borrow_mut();
                        let sender = sender.as_mut().unwrap();
                        let mut request = request_cpy.borrow_mut();
                        *request = Some(Box::pin(sender.send_request(req)));
                        let mut state = state_cpy.borrow_mut();
                        *state = RequestState::RequestSent;
                    });
                    *state = RequestState::SendingRequest;
                }
                RequestState::SendingRequest => (),
                RequestState::RequestSent => {
                    let waker = futures::task::noop_waker();
                    let mut cx = std::task::Context::from_waker(&waker);
                    let mut request = request.borrow_mut();
                    let request = request.as_mut().unwrap();
                    match request.as_mut().poll(&mut cx) {
                        Poll::Ready(Ok(response)) => {
                            *resp.borrow_mut() = Some(Box::pin(response));
                            *state = RequestState::StreamingBody;
                        }
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
                }
                RequestState::StreamingBody => {
                    let waker = futures::task::noop_waker();
                    let mut cx = std::task::Context::from_waker(&waker);
                    let mut resp = resp.borrow_mut();

                    // resp is never None here
                    let resp = resp.as_mut().unwrap();
                    match resp.frame().poll_unpin(&mut cx) {
                        Poll::Ready(Some(Ok(frame))) => {
                            if let Some(chunk) = frame.data_ref() {
                                let mut result = result.borrow_mut();
                                result.extend_from_slice(chunk);
                            }
                        }
                        // FIXME: throw exception instead of panic
                        Poll::Ready(Some(Err(e))) => panic!("error: {}", e),
                        Poll::Ready(None) => {
                            log::debug!("done");
                            *state = RequestState::Done;
                            let result = result.borrow_mut();
                            let body =
                                if let Some(encoding) = resp.headers().get("Content-Encoding") {
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

                            if let Ok(Some(authenticate_header)) = headers.get("www-authenticate") {
                                for val in authenticate_header.split(",") {
                                    let pair: Vec<&str> = val.split("=").collect();
                                    let val = &pair[1][1..pair[1].len() - 1];
                                    match pair[0].trim() {
                                        "Digest realm" => {
                                            *self_cpy.realm.borrow_mut() = val.to_owned()
                                        },
                                        "nonce" => {
                                            *self_cpy.nonce.borrow_mut() = val.to_owned()
                                        },
                                        "opaque" => {
                                            *self_cpy.opaque.borrow_mut() = val.to_owned()
                                        },
                                        "qop" => {
                                            if val != "auth" {
                                                log::error!("Got qop {val}, which is not supported");
                                            }
                                        }
                                        v => log::info!("Got unknown value in www-authenticate header: {}", v)
                                    }
                                }
                            }

                            let mut response_init = web_sys::ResponseInit::new();
                            response_init.status(resp.status().as_u16());
                            response_init.headers(&headers);
                            response_init
                                .status_text(resp.status().canonical_reason().unwrap_or(""));
                            let array: js_sys::Uint8Array = (&body[..]).into();
                            let response = web_sys::Response::new_with_opt_buffer_source_and_init(Some(&array), &response_init).unwrap();
                            let mut init = web_sys::CustomEventInit::new();
                            init.detail(&response.into());
                            let event = web_sys::CustomEvent::new_with_event_init_dict(
                                format!("get_{}", id).as_str(),
                                &init,
                            )
                            .unwrap();

                            let global = js_sys::global();
                            let global = web_sys::WorkerGlobalScope::from(JsValue::from(global));
                            global.dispatch_event(&event).unwrap();
                        }
                        Poll::Pending => (),
                    };
                    let mut conn = conn.borrow_mut();

                    // conn is never None here
                    let conn = conn.as_mut().unwrap();
                    match conn.as_mut().poll(&mut cx) {
                        Poll::Ready(Ok(_)) => (),
                        // FIXME: throw exception instead of panic
                        Poll::Ready(Err(e)) => panic!("error: {}", e),
                        Poll::Pending => (),
                    };
                }
                _ => {
                    let stream_cpy = stream_cpy.upgrade().unwrap();
                    let mut stream = stream_cpy.borrow_mut();
                    stream.close();
                    stream.poll();
                    if stream.is_open() {
                        return;
                    }
                    *current_request.upgrade().unwrap().borrow_mut() = None;
                    let request_queue = request_queue.upgrade().unwrap();
                    let mut request_queue = request_queue.borrow_mut();
                    if let Some(next) = request_queue.pop_front() {
                        wasm_bindgen_futures::spawn_local(async move {
                            self_cpy.fetch(next.js_request, next.url, Some(next.id), next.username, next.password);
                        })
                    }
                }
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

    pub fn disconnect_inner_ws(&mut self) {
        log::debug!("disconnecting inner ws");
        let mut sock_ref = self.websocket.borrow_mut();
        if let Some(socket) = &*sock_ref {
            socket.disconnect();
        }
        *sock_ref = None;
    }

    pub fn get_pcap_log(&self) -> Vec<u8> {
        self.pcap.borrow_mut().get_ref().to_owned()
    }
}

/**
    Simple struct to hold all relevant informations until WgClient is ready to process the next request.
*/
struct Request {
    pub id: f64,
    pub js_request: web_sys::Request,
    pub url: String,
    pub username: Option<String>,
    pub password: Option<String>,
}

/**
    Helper struct to be able to send Bytes as Body.
*/
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
This is a dummy error type that is never returned. It is needed because the Body trait needs an Error type.
*/
#[derive(Debug)]
struct DummyErr;

impl std::error::Error for DummyErr {}

impl std::fmt::Display for DummyErr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "how the f did this happen!?")
    }
}

#[cfg(test)]
pub mod test {
    use super::*;
    use wasm_bindgen_test::*;

    pub(self) fn create_wg_client(secret: &str, peer: &str, psk: &str, url: &str) -> WgClient {
        WgClient::new(secret, peer, psk, url, "", "", 80, js_sys::Function::new_no_args(""))
    }

    #[wasm_bindgen_test]
    fn test_create_wg_client() {
        let _ = create_wg_client(
            "EFHaYB4PvohMsO7VqxNQFyQhw6uKq6PD0FpjhZrCMkI=",
            "T1gy5yRSwYlSkjxAfnk/koNhlRyxsrFhdGW87LY1cxM=",
            "j4UDcamPDK+Cp0c2UT14sQPf4CQYE5DEQ52W5Mu4AmM=",
            "ws://localhost:8081",
        );
    }
}
