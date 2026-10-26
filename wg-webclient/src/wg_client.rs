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

use std::{cell::RefCell, collections::VecDeque, io::Read, pin::Pin, rc::{Rc, Weak}, task::Poll};

use crate::{
    hyper_stream::HyperStream, icmp_discovery::{self, PeerDiscovery},
    interface::Interface, interval_handle::IntervalHandle,
    stream::TcpStream, websocket::Websocket, wg_device::WgTunDevice,
};
use base64::Engine;
use boringtun::x25519;
use flate2::read::GzDecoder;
use futures::{Future, FutureExt};
use gloo_file::{File, ObjectUrl};
use http_body_util::BodyExt;
use hyper::{body::Bytes, client::conn::http1::{Connection, SendRequest}};
use js_sys::Promise;
use pcap_file::pcapng::PcapNgWriter;
use smoltcp::wire::IpAddress;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;
use web_sys::{CustomEvent, MessageEvent, Response};

type EventClosure = Closure<dyn FnMut(CustomEvent)>;
type EventQueue = Rc<RefCell<VecDeque<EventClosure>>>;
type HyperResp = hyper::Response<hyper::body::Incoming>;
type RequestFuture = Pin<Box<dyn Future<Output = hyper::Result<HyperResp>>>>;
type ConnPin = Pin<Box<Connection<HyperStream, Box<Body>>>>;
type RespPin = Pin<Box<HyperResp>>;

/// The exported client struct. It Wraps the actual Client and a Queue to keep the needed
/// callbacks alive.
/// Most function calls are simply passed to the wrapped object.
#[wasm_bindgen]
pub struct Client(
    WgClient,
    EventQueue,
);

#[wasm_bindgen]
impl Client {

    /// Use registered IPv4 or IPv6 tunnel addresses, preserving local prefixes.
    /// `discover_peer` defaults to false. With registered IPv4 addresses it
    /// probes the IPv6 discovery contract, falling back to those IPv4 addresses
    /// after three seconds without a reply. IPv6 discovery callers may supply
    /// an explicit IPv4 fallback pair. No management/outer address is consulted.
    #[wasm_bindgen(constructor)]
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        secret_str: &str,
        peer_str: &str,
        psk: &str,
        url: &str,
        internal_ip: &str,
        internal_peer_ip: &str,
        port: u16,
        mtu: usize,
        disconnect_cb: js_sys::Function,
        connect_cb: js_sys::Function,
        discover_peer: Option<bool>,
        fallback_local: Option<String>,
        fallback_peer: Option<String>,
    ) -> Self {
        console_log::init_with_level(log::Level::Debug).ok();
        Self(
            WgClient::new(secret_str, peer_str, psk, url, internal_ip, internal_peer_ip, port, mtu, disconnect_cb, connect_cb, discover_peer.unwrap_or(false), fallback_local.as_deref(), fallback_peer.as_deref()),
            Rc::new(RefCell::new(VecDeque::new())),
        )
    }


    /// Creates a new Promise that resolves when the event with the provided id is fired.
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
            let options = web_sys::AddEventListenerOptions::new();
            options.set_once(true);
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


    /// Makes a http request to the provided url and return a Promise that resolves to a JS Response object.
    /// Internally it calls the fetch function of the wrapped WgClient object and
    /// registers an EventListener for the event returned by it.
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

    /// Direct peers are immediately ready. Detection becomes ready after a
    /// correlated IPv6 reply or selection of the known IPv4 fallback.
    #[wasm_bindgen]
    pub fn peer_discovery_complete(&self) -> bool {
        self.0.peer_discovery_complete()
    }

    #[wasm_bindgen]
    pub fn peer_discovery_failed(&self) -> bool {
        self.0.discovery.as_ref().is_some_and(|d| d.borrow().failed)
    }

    /// Stop polling/discovery and close the outer tunnel before replacing or
    /// freeing this client. Unlike disconnect_inner_ws, this ends the session.
    #[wasm_bindgen]
    pub fn disconnect(&self) {
        self.0.clone().disconnect_inner_ws();
        self.0._polling_interval.cancel();
        self.0.current_request.borrow_mut().take();
        self.0.request_queue.borrow_mut().clear();
        self.0.iface.borrow().device().disconnect();
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


/// The struct that acutally does all the work
#[derive(Clone)]
struct WgClient {
    stream: Rc<RefCell<TcpStream<'static, WgTunDevice>>>,
    iface: Rc<RefCell<Interface<'static, WgTunDevice>>>,
    websocket: Rc<RefCell<Option<Websocket<WgTunDevice>>>>,
    current_request: Rc<RefCell<Option<IntervalHandle<MessageEvent>>>>,
    request_queue: Rc<RefCell<VecDeque<Request>>>,
    pcap: Rc<RefCell<PcapNgWriter<Vec<u8>>>>,
    /// Registered peer, or the result of explicitly enabled contract discovery.
    internal_peer_ip: Rc<RefCell<Option<IpAddress>>>,
    discovery: Option<Rc<RefCell<PeerDiscovery>>>,
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

    /// Creates a new object.
    #[allow(clippy::too_many_arguments)]
    fn new(
        secret_str: &str,
        peer_str: &str,
        psk: &str,
        url: &str,
        internal_ip: &str,
        internal_peer_ip: &str,
        port: u16,
        mtu: usize,
        disconnet_cb: js_sys::Function,
        connect_cb: js_sys::Function,
        discover_peer: bool,
        fallback_local: Option<&str>,
        fallback_peer: Option<&str>,
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
        let device = WgTunDevice::new(self_key, peer, psk, url, mtu, disconnet_cb, connect_cb).unwrap();

        let pcap = device.get_pcap();

        let cidr = icmp_discovery::parse_interface_cidr(internal_ip)
            .expect("Failed to parse internal_ip");
        let self_addr = cidr.address();
        let parsed_peer = icmp_discovery::parse_cidr_address(internal_peer_ip)
            .expect("Failed to parse internal_peer_ip");
        assert!(matches!((self_addr, parsed_peer),
            (IpAddress::Ipv4(_), IpAddress::Ipv4(_)) | (IpAddress::Ipv6(_), IpAddress::Ipv6(_))),
            "Local and peer tunnel addresses must have the same family");
        let iface = Rc::new(RefCell::new(Interface::new(device, cidr)));
        let iface_cpy = Rc::downgrade(&iface);

        let (self_addr, parsed_peer, fallback) = if discover_peer {
            match (self_addr, parsed_peer) {
                (IpAddress::Ipv4(_), IpAddress::Ipv4(peer)) => {
                    let mut bytes = [0; 16];
                    getrandom::fill(&mut bytes).unwrap();
                    bytes[0] = 0xfd;
                    let local = IpAddress::Ipv6(bytes.into());
                    iface.borrow_mut().add_ip_address(smoltcp::wire::IpCidr::new(local, 128));
                    (local, IpAddress::Ipv6(icmp_discovery::ALL_NODES_ADDRESS), Some(peer))
                }
                _ => {
                    let fallback = match (fallback_local, fallback_peer) {
                        (Some(local), Some(peer)) => {
                            let cidr = icmp_discovery::parse_interface_cidr(local).expect("Invalid fallback local address");
                            let peer = icmp_discovery::parse_cidr_address(peer).expect("Invalid fallback peer address");
                            let (IpAddress::Ipv4(_), IpAddress::Ipv4(peer)) = (cidr.address(), peer) else {
                                panic!("Fallback requires an IPv4 address pair");
                            };
                            iface.borrow_mut().add_ip_address(cidr);
                            Some(peer)
                        }
                        (None, None) => None,
                        _ => panic!("Fallback requires both local and peer addresses"),
                    };
                    (self_addr, parsed_peer, fallback)
                }
            }
        } else { (self_addr, parsed_peer, None) };

        // A public IPv6 listener says nothing about the inner tunnel family.
        // Use the registered peer directly unless the caller enables contract
        // detection, with a known IPv4 fallback where available.
        let (internal_peer_ip, icmp_handle, discovery) = if discover_peer {
            assert!(matches!(parsed_peer, IpAddress::Ipv6(_)), "Discovery requires an IPv6 peer");
            let buffer = || smoltcp::socket::icmp::PacketBuffer::new(
                vec![smoltcp::socket::icmp::PacketMetadata::EMPTY; 4], vec![0; 1024],
            );
            let handle = iface.borrow_mut().set_icmp_socket(buffer(), buffer());
            icmp_discovery::bind_discovery_socket(&mut iface.borrow_mut(), handle);
            let mut nonce = [0; 16];
            getrandom::fill(&mut nonce).unwrap();
            let mut discovery = PeerDiscovery::new(crate::utils::now(), nonce);
            if let Some(peer) = fallback { discovery = discovery.with_ipv4_fallback(peer); }
            (Rc::new(RefCell::new(None)), Some(handle),
                Some(Rc::new(RefCell::new(discovery))))
        } else {
            (Rc::new(RefCell::new(Some(parsed_peer))), None, None)
        };

        let stream = TcpStream::new(iface.clone());

        let stream = Rc::new(RefCell::new(stream));

        let peer_weak = Rc::downgrade(&internal_peer_ip);
        let discovery_weak = discovery.as_ref().map(Rc::downgrade);
        let closure = Closure::<dyn FnMut(_)>::new(move |_: MessageEvent| {
            let Some(iface) = iface_cpy.upgrade() else { return; };
            iface.borrow_mut().poll();
            let (Some(handle), Some(discovery), Some(peer)) = (
                icmp_handle, discovery_weak.as_ref().and_then(Weak::upgrade), peer_weak.upgrade(),
            ) else { return; };
            if peer.borrow().is_some() { return; }
            if let IpAddress::Ipv6(local) = self_addr {
                let mut iface = iface.borrow_mut();
                if let Some(address) = discovery.borrow_mut().poll(
                    &mut iface, handle, local, crate::utils::now(),
                ) {
                    iface.remove_socket(handle);
                    iface.select_address_family(address);
                    log::info!("User tunnel selected peer {address}");
                    *peer.borrow_mut() = Some(address);
                }
            }
        });
        let polling_interval = Rc::new(IntervalHandle::new(closure, 0));

        Self {
            stream,
            iface,
            websocket: Rc::new(RefCell::new(None)),
            current_request: Rc::new(RefCell::new(None)),
            request_queue: Rc::new(RefCell::new(VecDeque::new())),
            pcap,
            internal_peer_ip,
            discovery,
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

    /// Creates a new Websocket object and connection that gets stored internally.
    fn start_inner_ws(&mut self, cb: js_sys::Function) {
        self.disconnect_inner_ws();
        // Only enabled multicast discovery needs to wait.
        let Some(peer) = self.internal_peer_ip.borrow().clone() else {
            log::debug!(
                "Skipping websocket setup: peer address not yet discovered via ICMPv6"
            );
            return;
        };
        self.connect_websocket(peer, cb);
    }

    /// Direct or successfully selected peers are ready, regardless of family.
    pub fn peer_discovery_complete(&self) -> bool {
        self.internal_peer_ip.borrow().is_some()
    }

    /// Connect the inner WebSocket using the given peer address and store the
    /// resulting WebSocket, using the registered or explicitly discovered peer.
    fn connect_websocket(&mut self, peer: IpAddress, cb: js_sys::Function) {
        let mut stream = TcpStream::new(self.iface.clone());
        let out_port = js_sys::Math::random() * 1000.0;
        let out_port = out_port as u16;
        let endpoint = smoltcp::wire::IpEndpoint::new(peer, self.port);
        if let Err(err) = stream.connect(endpoint, out_port) {
            log::error!("Error when connecting websocket: {}", err);
        }
        let auth_header = if !self.nonce.borrow().is_empty() {
            Some(self.build_authentication_header("GET", "/ws"))
        } else {
            None
        };
        let mut websocket = Websocket::connect(stream, auth_header).unwrap();
        websocket.on_message(cb);
        *self.websocket.borrow_mut() = Some(websocket);
    }

    /// Builds the authentication header for the http requests.
    fn build_authentication_header(&self, method: &str, uri: &str) ->  String {
        let ha1 = md5::compute(format!("{}:{}:{}", self.username.borrow(), self.realm.borrow(), self.password.borrow()));
        let ha2 = md5::compute(format!("{}:{}", method, uri));

        let this: JsValue = js_sys::global().into();
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

        let req = self.build_request_closure(url, js_request, id);
        let interval = IntervalHandle::new(req, 0);
        *self.current_request.borrow_mut() = Some(interval);

        format!("get_{}", id)
    }

    /// Builds the closure that gets called by the interval built in the fetch function
    fn build_request_closure(&self, url: String, js_request: web_sys::Request, id: f64) -> Closure<dyn FnMut(MessageEvent)> {
        let state = Rc::new(RefCell::new(RequestState::Begin));
        let stream = Rc::downgrade(&self.stream);

        let sender = Rc::new(RefCell::new(None));
        let conn = Rc::new(RefCell::new(None));
        let request = Rc::new(RefCell::new(None));
        let resp = Rc::new(RefCell::new(None));

        let result = Rc::new(RefCell::new(vec![0u8; 0]));

        let url = Rc::new(url);
        let js_request = Rc::new(js_request);

        let request_queue = Rc::downgrade(&self.request_queue);
        let current_request = Rc::downgrade(&self.current_request);
        let self_cpy = self.clone();
        // We cannot move the `RefCell` into the closure directly because it is
        // not `Sync`, so we keep a `Weak` reference and re-borrow inside the
        // closure. This way, the most up-to-date discovered address is used
        // for every reconnect attempt.
        let internal_peer_ip = Rc::downgrade(&self.internal_peer_ip);

        let port = self.port;
        Closure::<dyn FnMut(_)>::new(move |_: MessageEvent| {
            if !stream.upgrade().unwrap().borrow().is_up() {
                return;
            }
            let mut state_ref = state.borrow_mut();
            let self_cpy = self_cpy.clone();

            match *state_ref {
                RequestState::Begin => {
                    let out_port = js_sys::Math::random() * 1000.0;
                    let out_port = out_port as u16;
                    // The discovered address may not be available yet for the
                    // IPv6 path; skip the connect attempt until then.
                    let Some(internal_peer_ip) = internal_peer_ip
                        .upgrade()
                        .and_then(|rc| rc.borrow().clone())
                    else {
                        log::debug!(
                            "Skipping connect: peer address not yet discovered via ICMPv6"
                        );
                        return;
                    };
                    let endpoint = smoltcp::wire::IpEndpoint::new(internal_peer_ip, port);

                    // FIXME: throw exception instead of panic
                    if let Err(err) = stream.upgrade().unwrap().borrow_mut().connect(endpoint, out_port) {
                        log::error!("Error connecting to endpoint {}: {}", endpoint, err);
                        return;
                    }
                    *state_ref = RequestState::Started;
                }
                RequestState::Started => {
                    let stream = stream.upgrade().unwrap();
                    let stream = stream.borrow_mut();
                    if stream.can_send() {
                        *state_ref = RequestState::Connected;
                    }
                }
                RequestState::Connected => {
                    let is_none = sender.borrow_mut().is_none();

                    if is_none {
                        setup_connection(Rc::downgrade(&sender), Rc::downgrade(&conn), Rc::downgrade(&state), stream.clone());
                    }
                }
                RequestState::HandshakeDone => {
                    send_http_reqeust(js_request.clone(), url.clone(), Rc::downgrade(&sender), Rc::downgrade(&request), Rc::downgrade(&state), self_cpy);
                    *state_ref = RequestState::SendingRequest;
                }
                RequestState::SendingRequest => (),
                RequestState::RequestSent => {
                    poll_request(request.clone(), resp.clone(), state_ref, conn.clone());
                }
                RequestState::StreamingBody => {
                    poll_response(resp.clone(), result.clone(), state_ref, self_cpy, id, conn.clone());
                }
                _ => {
                    let stream = stream.upgrade().unwrap();
                    let mut stream = stream.borrow_mut();
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
        })
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
        element.set_attribute("href", &file).unwrap();
        element.set_attribute("target", "_blank").unwrap();
        let element = wasm_bindgen::JsValue::from(element);
        let element = web_sys::HtmlElement::from(element);
        element.click();
    }

    pub fn disconnect_inner_ws(&mut self) {
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


/// Transforms the raw tcp stream into sender and receiver objects for hyper.
fn setup_connection(sender: Weak<RefCell<Option<SendRequest<Box<Body>>>>>, conn: Weak<RefCell<Option<ConnPin>>>, state: Weak<RefCell<RequestState>>, stream: Weak<RefCell<TcpStream<'static, WgTunDevice>>>) {
    wasm_bindgen_futures::spawn_local(async move {
        let stream = HyperStream::new(stream.upgrade().unwrap().clone());

        // FIXME: throw exception instead of panic
        let (send_request, new_conn) =
            hyper::client::conn::http1::handshake(stream).await.unwrap();
        *sender.upgrade().unwrap().borrow_mut() = Some(send_request);
        *conn.upgrade().unwrap().borrow_mut() = Some(Box::pin(new_conn));
        *state.upgrade().unwrap().borrow_mut() = RequestState::HandshakeDone;
    });
}


/// Reads the request coming from js, converts it to a hyper request and sends it.
fn send_http_reqeust(
    js_request: Rc<web_sys::Request>,
    url: Rc<String>,
    sender: Weak<RefCell<Option<SendRequest<Box<Body>>>>>,
    request: Weak<RefCell<Option<RequestFuture>>>,
    state: Weak<RefCell<RequestState>>,
    self_cpy: WgClient,
) {
    wasm_bindgen_futures::spawn_local(async move {
        let body = JsFuture::from(js_request.array_buffer().unwrap())
            .await
            .unwrap();
        let body = js_sys::Uint8Array::new(&body).to_vec();
        let method_str = js_request.method();
        let method = match method_str.as_str() {
            "GET" => hyper::Method::GET,
            "PUT" => hyper::Method::PUT,
            "POST" => hyper::Method::POST,
            "DELETE" => hyper::Method::DELETE,
            // FIXME: throw exception instead of panic
            _ => panic!("unknown method: {}", js_request.method()),
        };

        let mut req = http::Request::builder().method(method);

        let headers = js_request.headers();
        for key in js_sys::Object::keys(&headers).iter() {
            let key = key.as_string().unwrap();
            let value = if let Ok(Some(value)) = headers.get(&key) {
                value
            } else {
                continue;
            };
            req = req.header(key, value);
        }

        if !self_cpy.nonce.borrow().is_empty() {
            let auth_header = self_cpy.build_authentication_header(&method_str, &url);
            req = req.header("Authorization", auth_header);
        }

        let req = req
            .header("Content-Type", "application/json; charset=utf-8")
            .uri(url.as_str())
            .body(Box::new(Body::new(Bytes::copy_from_slice(&body))))
            .unwrap();
        let sender = sender.upgrade().unwrap();
        let mut sender = sender.borrow_mut();
        let sender = sender.as_mut().unwrap();
        let request = request.upgrade().unwrap();
        let mut request = request.borrow_mut();
        *request = Some(Box::pin(sender.send_request(req)));
        *state.upgrade().unwrap().borrow_mut() = RequestState::RequestSent;
    });
}


/// Polls the sending end of the hyper connection until the request is sent.
fn poll_request(
    request: Rc<RefCell<Option<RequestFuture>>>,
    resp: Rc<RefCell<Option<RespPin>>>,
    mut state: std::cell::RefMut<'_, RequestState>,
    conn: Rc<RefCell<Option<ConnPin>>>,
) {
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


/// Collects the raw response stream and parses it into a Response object.
fn poll_response(
    resp: Rc<RefCell<Option<RespPin>>>,
    result: Rc<RefCell<Vec<u8>>>,
    mut state_ref: std::cell::RefMut<'_, RequestState>,
    self_cpy: WgClient,
    id: f64,
    conn: Rc<RefCell<Option<ConnPin>>>,
) {
    let waker = futures::task::noop_waker();
    let mut cx = std::task::Context::from_waker(&waker);
    let mut resp = resp.borrow_mut();

    // resp is never None here
    let resp = resp.as_mut().unwrap();
    match resp.frame().poll_unpin(&mut cx) {

        // Just append the data to the result while there is is something to read
        // FIXME: handle Content-Length
        Poll::Ready(Some(Ok(frame))) => {
            if let Some(chunk) = frame.data_ref() {
                let mut result = result.borrow_mut();
                result.extend_from_slice(chunk);
            }
        }
        // FIXME: throw exception instead of panic
        Poll::Ready(Some(Err(e))) => panic!("error: {}", e),

        // Parse the data when done reading it
        Poll::Ready(None) => {
            log::debug!("done");
            *state_ref = RequestState::Done;
            let result = result.borrow_mut();
            let body =
                if let Some(encoding) = resp.headers().get("Content-Encoding") {
                    if encoding == "gzip" {
                        let mut gz = GzDecoder::new(&result[..]);
                        let mut s = String::new();
                        if let Err(err) = gz.read_to_string(&mut s) {
                            log::error!("Error decoding gzip: {}", err);
                        }
                        s.as_bytes().to_vec()
                    } else if encoding == "deflate" {
                        let mut gz = flate2::read::ZlibDecoder::new(&result[..]);
                        let mut s = String::new();
                        if let Err(err) = gz.read_to_string(&mut s) {
                            log::error!("Error decoding deflate: {}", err);
                        }
                        s.as_bytes().to_vec()
                    } else {
                        log::error!("Unknown encoding: {}", encoding.to_str().unwrap());
                        result.clone()
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

            // Build the actual response object
            let response_init = web_sys::ResponseInit::new();
            response_init.set_status(resp.status().as_u16());
            response_init.set_headers(&headers);
            response_init
                .set_status_text(resp.status().canonical_reason().unwrap_or(""));
            let array: js_sys::Uint8Array = (&body[..]).into();
            let response = web_sys::Response::new_with_opt_buffer_source_and_init(Some(&array), &response_init).unwrap();
            let init = web_sys::CustomEventInit::new();
            init.set_detail(&response.into());
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

#[cfg(all(test, target_arch = "wasm32"))]
mod test {
    use super::*;
    use wasm_bindgen_test::*;

    async fn delay() {
        let promise = Promise::new(&mut |resolve, _| {
            let global = web_sys::WorkerGlobalScope::from(JsValue::from(js_sys::global()));
            global.set_timeout_with_callback_and_timeout_and_arguments_0(&resolve, 20).unwrap();
        });
        JsFuture::from(promise).await.unwrap();
    }

    async fn wait_for(mut ready: impl FnMut() -> bool) {
        for _ in 0..250 {
            if ready() { return; }
            delay().await;
        }
        panic!("test peer did not respond within 5 seconds");
    }

    // Requires: cargo run --target x86_64-unknown-linux-gnu --example tunnel_peer
    async fn exercise(outer: &str, local: &str, peer: &str, discover: bool, fallback: Option<(&str, &str)>) {
        crate::set_pcap_logging(true);
        let expect_ipv6 = outer.contains("/v6");
        let mut previous_local = None;
        let mut previous_peer = None;
        // Recreate the complete client to cover reconnection with new WG sessions.
        for _ in 0..2 {
            let local = if discover && local.contains(':') {
                let mut address = [0; 16];
                getrandom::fill(&mut address).unwrap();
                address[0] = 0xfd;
                format!("{}/128", std::net::Ipv6Addr::from(address))
            } else { local.to_owned() };
            let noop = Closure::<dyn FnMut()>::new(|| {});
            let engine = base64::engine::general_purpose::STANDARD;
            let client = Client::new(
                &engine.encode([1; 32]),
                &engine.encode(x25519::PublicKey::from(&x25519::StaticSecret::from([2; 32])).as_bytes()),
                &engine.encode([3; 32]), outer, &local, peer, 80, 1420,
                noop.as_ref().unchecked_ref::<js_sys::Function>().clone(),
                noop.as_ref().unchecked_ref::<js_sys::Function>().clone(), Some(discover),
                fallback.map(|(local, _)| local.to_owned()), fallback.map(|(_, peer)| peer.to_owned()),
            );
            assert_eq!(client.peer_discovery_complete(), !discover);
            wait_for(|| client.0.iface.borrow().is_up() && client.peer_discovery_complete()).await;
            if discover && expect_ipv6 {
                let discovered = client.0.internal_peer_ip.borrow().unwrap();
                let IpAddress::Ipv6(address) = discovered else { panic!("discovered IPv4 peer"); };
                assert_eq!(address.octets()[0], 0xfd);
                assert_ne!(previous_peer, Some(discovered));
                previous_peer = Some(discovered);
            }
            let request = web_sys::Request::new_with_str("http://test-peer/status").unwrap();
            let response = client.fetch(request, "/status".into(), None, None).await;
            assert_eq!(response.status(), 200);
            let body = JsFuture::from(response.text().unwrap()).await.unwrap().as_string().unwrap();
            assert_eq!(body, if expect_ipv6 { "peer-v6" } else { "peer-v4" });
            let messages = Rc::new(RefCell::new(Vec::new()));
            let received = messages.clone();
            let callback = Closure::<dyn FnMut(String)>::new(move |message| received.borrow_mut().push(message));
            client.start_inner_ws(callback.as_ref().unchecked_ref::<js_sys::Function>().clone());
            wait_for(|| !messages.borrow().is_empty()).await;
            assert_eq!(&*messages.borrow(), &["peer-ws"]);
            let source = assert_capture(&client, fallback.filter(|_| !expect_ipv6).map(|p| p.0).unwrap_or(&local), discover, expect_ipv6);
            if discover {
                assert_ne!(previous_local, source, "replacement reused the IPv6 probe source");
                previous_local = source;
            }
            client.disconnect();
            drop(client);
        }
    }

    fn assert_capture(client: &Client, local: &str, discover: bool, expect_ipv6: bool) -> Option<smoltcp::wire::Ipv6Address> {
        use pcap_file::pcapng::{Block, PcapNgReader};
        use smoltcp::wire::{Ipv4Packet, Ipv6Packet, IpProtocol};

        let content = client.get_pcap_log();
        let mut reader = PcapNgReader::new(content.as_slice()).unwrap();
        let mut interfaces = 0;
        let mut packets = 0;
        let mut discovery_requests = 0;
        let mut http_requests = 0;
        let mut ws_requests = 0;
        let mut probe_source = None;
        let selected_peer = client.0.internal_peer_ip.borrow().unwrap();
        while let Some(block) = reader.next_block() {
            match block.unwrap() {
                Block::InterfaceDescription(interface) => {
                    interfaces += 1;
                    assert_eq!(interface.linktype, pcap_file::DataLink::RAW);
                }
                Block::EnhancedPacket(packet) => {
                    packets += 1;
                    assert_eq!(packet.interface_id, 0);
                    let (src, dst, protocol) = if packet.data[0] >> 4 == 6 {
                        let ip = Ipv6Packet::new_checked(packet.data.as_ref()).unwrap();
                        if ip.next_header() == IpProtocol::Icmpv6 && ip.payload()[0] == 128 {
                            discovery_requests += 1;
                            assert!(discover);
                            assert_eq!(ip.src_addr().octets()[0], 0xfd);
                            probe_source = Some(ip.src_addr());
                            assert_eq!(ip.dst_addr(), icmp_discovery::ALL_NODES_ADDRESS);
                        }
                        (IpAddress::Ipv6(ip.src_addr()), IpAddress::Ipv6(ip.dst_addr()), ip.next_header())
                    } else {
                        let ip = Ipv4Packet::new_checked(packet.data.as_ref()).unwrap();
                        (IpAddress::Ipv4(ip.src_addr()), IpAddress::Ipv4(ip.dst_addr()), ip.next_header())
                    };
                    if protocol == IpProtocol::Tcp {
                        assert_eq!(packet.data[0] >> 4, if expect_ipv6 { 6 } else { 4 });
                        let selected_local = if expect_ipv6 && discover { IpAddress::Ipv6(probe_source.unwrap()) }
                            else { icmp_discovery::parse_cidr_address(local).unwrap() };
                        assert!((src == selected_local && dst == selected_peer) || (dst == selected_local && src == selected_peer));
                        if packet.data.windows(12).any(|bytes| bytes == b"GET /status ") { http_requests += 1; }
                        if packet.data.windows(8).any(|bytes| bytes == b"GET /ws ") { ws_requests += 1; }
                    }
                }
                _ => (),
            }
        }
        assert_eq!(interfaces, 1);
        assert!(packets > 0);
        assert!(http_requests > 0);
        assert!(ws_requests > 0);
        if discover { assert!(discovery_requests > 0); }
        probe_source
    }

    #[wasm_bindgen_test(async)]
    async fn ipv4_tunnel_over_ipv4_browser_connection() {
        exercise("ws://127.0.0.1:18081/v4", "10.123.1.3", "10.123.1.2", false, None).await;
    }

    #[wasm_bindgen_test(async)]
    async fn ipv4_tunnel_over_ipv6_browser_connection() {
        exercise("ws://[::1]:18081/v4", "10.123.1.3/24", "10.123.1.2/32", false, None).await;
    }

    #[wasm_bindgen_test(async)]
    async fn registered_ipv6_peer_with_64_prefix() {
        exercise("ws://[::1]:18081/v6", "fd00::3/64", "fd00::2/64", false, None).await;
    }

    #[wasm_bindgen_test(async)]
    async fn registered_ipv6_peer_with_128_prefix() {
        exercise("ws://127.0.0.1:18081/v6", "fd00::3/128", "fd00::2/128", false, None).await;
    }

    #[wasm_bindgen_test(async)]
    async fn ipv6_discovery_contract_http_websocket_and_reconnect() {
        exercise("ws://[::1]:18081/v6-discovery", "fd00::3/128", "ff02::1", true, None).await;
    }

    #[wasm_bindgen_test(async)]
    async fn stale_ipv4_registration_detects_ipv6_user_peer() {
        exercise("ws://[::1]:18081/v6-discovery", "10.123.1.3/24", "10.123.1.2/32", true, None).await;
    }

    #[wasm_bindgen_test(async)]
    async fn automatic_detection_falls_back_to_registered_ipv4() {
        exercise("ws://[::1]:18081/v4", "10.123.1.3/24", "10.123.1.2/32", true, None).await;
    }

    #[wasm_bindgen_test(async)]
    async fn discovery_token_can_fall_back_after_firmware_downgrade() {
        exercise("ws://127.0.0.1:18081/v4", "fd00::3/128", "fe80::1", true,
            Some(("10.123.1.3/24", "10.123.1.2/32"))).await;
    }
}
