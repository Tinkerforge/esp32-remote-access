
use std::{cell::RefCell, collections::VecDeque, io::Read, rc::Rc, task::Poll};

use futures::{Future, FutureExt};
use gloo_file::{File, ObjectUrl};
use http_body_util::BodyExt;
use hyper::body::Bytes;
use js_sys::Promise;
use pcap_file::pcapng::PcapNgWriter;
use smoltcp::wire::{IpAddress, IpCidr};
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;
use web_sys::{MessageEvent, CustomEvent};
use crate::{hyper_stream::HyperStream, interface::Interface, interval_handle::IntervalHandle, stream::TcpStream, websocket::Websocket, wg_device::WgTunDevice};
use base64::Engine;
use boringtun::x25519;
use flate2::read::GzDecoder;

use crate::console_log;

/**
 * The exported client struct. It Wraps the actual Client and a Queue to keep the needed
 * callbacks alive.
 * Most function calls are simply passed to the wrapped object.
 */
#[wasm_bindgen]
pub struct Client(WgClient, Rc<RefCell<VecDeque<Closure<dyn FnMut(CustomEvent)>>>>);

#[wasm_bindgen]
impl Client {
    /**
     * Creates a new Client struct by also creating the wrapped objects.
     */
    #[wasm_bindgen(constructor)]
    pub fn new(secret_str: &str, peer_str: &str, url: &str) -> Self {
        Self(WgClient::new(secret_str, peer_str, url), Rc::new(RefCell::new(VecDeque::new())))
    }

    /**
     * Makes a http request to the provided url and return a Promise that resolves to a JS Response object.
     * Internally it calls the fetch function of the wrapped WgClient object and
     * registers an EventListener for the event returned by it.
     */
    #[wasm_bindgen]
    pub fn fetch(&self, request: web_sys::Request, url: String) -> Promise {
        let cpy = self.0.clone();
        let id = cpy.fetch(request, url, None);
        Promise::new(&mut move |resolve, _| {
            let queue = self.1.clone();
            let closure = Closure::<dyn FnMut(_)>::new(move |event: CustomEvent| {
                let response = event.detail();
                let _ = queue.borrow_mut().pop_front();
                resolve.call1(&JsValue::NULL, &response).unwrap();
            });

            // add event listener should not fail
            let global = js_sys::global();
            let global = web_sys::WorkerGlobalScope::from(JsValue::from(global));
            let mut options = web_sys::AddEventListenerOptions::new();
            options.once(true);
            global.add_event_listener_with_callback_and_add_event_listener_options(id.as_str(), closure.as_ref().unchecked_ref(), &options).unwrap();
            self.1.borrow_mut().push_back(closure);
        })
    }

    #[wasm_bindgen]
    pub fn start_ws(&mut self) {
        self.0.start_ws();
    }

    #[wasm_bindgen]
    pub fn on_message(&mut self, cb: js_sys::Function) {
        self.0.websocket.as_ref().unwrap().borrow_mut().on_message(cb);
    }

    #[wasm_bindgen]
    pub fn disconnect_ws(&mut self) {
        self.0.disconnect_ws();
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
    websocket: Option<Rc<RefCell<Websocket<WgTunDevice>>>>,
    current_request: Rc<RefCell<Option<IntervalHandle<MessageEvent>>>>,
    request_queue: Rc<RefCell<VecDeque<Request>>>,
    pcap: Rc<RefCell<PcapNgWriter<Vec<u8>>>>,
}

enum RequestState {
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
    fn new(secret_str: &str, peer_str: &str, url: &str) -> Self {
        console_error_panic_hook::set_once();

        let mut secret = [0u8; 32];
        let engine = base64::engine::general_purpose::STANDARD;

        // decoding secret and peer public key should fail very noticably. So either panic or throw exception
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

        // same as above
        let device = WgTunDevice::new(
            self_key,
            peer,
            url
        ).unwrap();

        let pcap = device.get_pcap();

        let ip = IpCidr::new(IpAddress::v4(123, 123, 123, 3), 24);
        let iface = Rc::new(RefCell::new(Interface::new(device, ip)));
        let iface_cpy = iface.clone();

        let stream = TcpStream::new(iface.clone());

        let stream = Rc::new(RefCell::new(stream));

        let global = js_sys::global();
        let global = web_sys::WorkerGlobalScope::from(JsValue::from(global));
        let closure = Closure::<dyn FnMut(_)>::new(move |_: MessageEvent| {
            iface_cpy.borrow_mut().poll();
        });
        global.set_interval_with_callback_and_timeout_and_arguments_0(
            closure.as_ref().unchecked_ref(),
            0,
        ).unwrap();
        closure.forget();

        Self {
            stream,
            iface,
            websocket: None,
            current_request: Rc::new(RefCell::new(None)),
            request_queue: Rc::new(RefCell::new(VecDeque::new())),
            pcap,
        }
    }

    /**
     * Creates a new Websocket object and connection that gets stored internally.
     */
    fn start_ws(&mut self) {
        let mut stream = TcpStream::new(self.iface.clone());
        let port = js_sys::Math::random() * 1000.0;
        let port = port as u16;
        let endpoint = smoltcp::wire::IpEndpoint::new(smoltcp::wire::IpAddress::v4(123, 123, 123, 2), 80);
        stream.connect(endpoint, port).unwrap_or_else(|e| {
            console_log!("{}", e);
            return
        });
        let websocket = Websocket::connect(stream).unwrap();
        self.websocket = Some(Rc::new(RefCell::new(websocket)));
    }

    /**
      The function that actually does the http requests.
      Since we must not block the thread while waiting for the response because it would also block
      the underlaying network stack it starts a task with setTimeout that polls the so far received
      response, fires a custom event when finished and either returns or proceeds with the next
      request.
     */
    fn fetch(&self, js_request: web_sys::Request, url: String, in_id: Option<f64>) -> String {
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
                url
            });
            return format!("get_{}", id);
        }

        {
            let port = js_sys::Math::random() * 1000.0;
            let port = port as u16;
            let endpoint = smoltcp::wire::IpEndpoint::new(smoltcp::wire::IpAddress::v4(123, 123, 123, 2), 80);

            // FIXME: throw exception instead of panic
            self.stream.borrow_mut().connect(endpoint, port).unwrap();
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

        let url = Rc::new(url);
        let js_request = Rc::new(js_request);

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
                    let is_none = sender.borrow_mut().is_none();

                    if is_none {
                        let sender_cpy = sender_cpy.clone();
                        let conn_cpy = conn_cpy.clone();
                        let stream_cpy = stream_cpy.clone();
                        let state_cpy = state_cpy.clone();
                        wasm_bindgen_futures::spawn_local(async move {
                            let stream = HyperStream::new(stream_cpy.clone());

                            // FIXME: throw exception instead of panic
                            let (sender, conn) = hyper::client::conn::http1::handshake(stream).await.unwrap();
                            *sender_cpy.borrow_mut() = Some(sender);
                            *conn_cpy.borrow_mut() = Some(Box::pin(conn));
                            *state_cpy.borrow_mut() = RequestState::HandshakeDone;
                        });
                    }
                },
                RequestState::HandshakeDone => {
                    console_log!("Handshake done");
                    let js_request_cpy = js_request.clone();
                    let sender_cpy = sender.clone();
                    let request_cpy = request.clone();
                    let state_cpy = state_cpy.clone();
                    let url_cpy = url.clone();
                    wasm_bindgen_futures::spawn_local(async move {
                        console_log!("start async part");
                        let body = JsFuture::from(js_request_cpy.array_buffer().unwrap()).await.unwrap();
                        let body = js_sys::Uint8Array::new(&body).to_vec();
                        let method = match js_request_cpy.method().as_str() {
                            "GET" => hyper::Method::GET,
                            "PUT" => hyper::Method::PUT,
                            "POST" => hyper::Method::POST,
                            // FIXME: throw exception instead of panic
                            _ => panic!("unknown method: {}", js_request_cpy.method()),
                        };

                        let mut req = http::Request::builder()
                        .method(method);

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

                        console_log!("!!!!!!!!!!!! body size: {}", body.len());
                        let req = req.header("Content-Type", "application/json; charset=utf-8")
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
                },
                RequestState::SendingRequest => (),
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

                    // resp is never None here
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

                            let global = js_sys::global();
                            let global = web_sys::WorkerGlobalScope::from(JsValue::from(global));
                            global.dispatch_event(&event).unwrap();
                        },
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
                },
                _ => {
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
                            self_cpy.fetch(next.js_request, next.url, Some(next.id));
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

    pub fn disconnect_ws(&mut self) {
        if let Some(socket) = self.websocket.clone() {
            let socket = socket.borrow_mut();
            socket.disconnect();
            self.websocket = None;
        }
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
    pub url: String
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
    use wasm_bindgen_test::*;
    use super::*;

    pub(self) fn create_wg_client(secret: &str, peer: &str, url: &str) -> WgClient {
        WgClient::new(secret, peer, url)
    }

    #[wasm_bindgen_test]
    fn test_create_wg_client() {
        let _ = create_wg_client("EFHaYB4PvohMsO7VqxNQFyQhw6uKq6PD0FpjhZrCMkI=", "T1gy5yRSwYlSkjxAfnk/koNhlRyxsrFhdGW87LY1cxM=", "ws://localhost:8081");
    }
}
