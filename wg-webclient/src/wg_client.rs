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

use std::{
    cell::RefCell,
    io::Read,
    pin::pin,
    rc::{Rc, Weak},
    task::Poll,
};

use crate::{
    hyper_stream::HyperStream,
    icmp_discovery::{self, PeerDiscovery},
    interface::Interface,
    interval_handle::IntervalHandle,
    stream::TcpStream,
    websocket::Websocket,
    wg_device::WgTunDevice,
};
use base64::Engine;
use boringtun::x25519;
use flate2::read::{GzDecoder, ZlibDecoder};
use futures::{
    channel::oneshot,
    future::{poll_fn, select, Either, Shared},
    task::AtomicWaker,
    FutureExt,
};
use gloo_file::{File, ObjectUrl};
use http_body_util::{BodyExt, Full};
use hyper::body::Bytes;
use js_sys::Promise;
use pcap_file::pcapng::PcapNgWriter;
use smoltcp::wire::{IpAddress, IpEndpoint};
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;
use web_sys::{MessageEvent, Response};

struct HttpRequests {
    tail: RefCell<Option<oneshot::Receiver<()>>>,
    disconnect: RefCell<Option<oneshot::Sender<()>>>,
    disconnected: Shared<oneshot::Receiver<()>>,
    waker: AtomicWaker,
}

impl HttpRequests {
    fn new() -> Self {
        let (disconnect, disconnected) = oneshot::channel();
        Self {
            tail: RefCell::new(None),
            disconnect: RefCell::new(Some(disconnect)),
            disconnected: disconnected.shared(),
            waker: AtomicWaker::new(),
        }
    }
}

#[wasm_bindgen]
pub struct Client(WgClient);

#[wasm_bindgen]
impl Client {

    /// IPv6 always uses a fresh random link-local address and peer discovery.
    /// `discover_peer` defaults to false for registered IPv4 addresses. Enabling
    /// it probes IPv6, falling back to that IPv4 pair and prefix after three
    /// seconds without a reply. IPv6 callers may supply an explicit IPv4
    /// fallback pair. Saved IPv6 addresses are not used as service endpoints.
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
        Self(WgClient::new(
            secret_str,
            peer_str,
            psk,
            url,
            internal_ip,
            internal_peer_ip,
            port,
            mtu,
            disconnect_cb,
            connect_cb,
            discover_peer.unwrap_or(false),
            fallback_local.as_deref(),
            fallback_peer.as_deref(),
        ))
    }

    /// Sends an HTTP request through the tunnel and resolves to a JS Response.
    #[wasm_bindgen(unchecked_return_type = "Promise<Response>")]
    pub fn fetch(
        &self,
        request: web_sys::Request,
        url: String,
        username: Option<String>,
        password: Option<String>,
    ) -> Promise {
        // An exported async &self method holds a wasm-bindgen borrow until its
        // promise settles. Own the client state so reconnect can free the
        // exported object while HTTP requests are still waiting.
        let client = self.0.clone();
        wasm_bindgen_futures::future_to_promise(async move {
            let response = client
                .fetch(
                    request.clone()?,
                    &url,
                    username.as_deref(),
                    password.as_deref(),
                )
                .await?;
            let response = if response.status() == 401 {
                client
                    .fetch(request, &url, username.as_deref(), password.as_deref())
                    .await?
            } else {
                response
            };
            Ok(response.into())
        })
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
        // Closing the shared signal cancels the active request and every queued request.
        self.0.http.disconnect.borrow_mut().take();
        self.0.http.waker.take();
        self.0.clone().disconnect_inner_ws();
        self.0._polling_interval.cancel();
        self.0.stream.borrow_mut().close();
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
    http: Rc<HttpRequests>,
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
        let discover_peer = discover_peer || matches!(parsed_peer, IpAddress::Ipv6(_));
        let (cidr, parsed_peer, fallback) = if discover_peer {
            let fallback = match (self_addr, parsed_peer) {
                (IpAddress::Ipv4(_), IpAddress::Ipv4(peer)) => Some((cidr, peer)),
                _ => match (fallback_local, fallback_peer) {
                    (Some(local), Some(peer)) => {
                        let cidr = icmp_discovery::parse_interface_cidr(local).expect("Invalid fallback local address");
                        let peer = icmp_discovery::parse_cidr_address(peer).expect("Invalid fallback peer address");
                        let (IpAddress::Ipv4(_), IpAddress::Ipv4(peer)) = (cidr.address(), peer) else {
                            panic!("Fallback requires an IPv4 address pair");
                        };
                        Some((cidr, peer))
                    }
                    (None, None) => None,
                    _ => panic!("Fallback requires both local and peer addresses"),
                },
            };
            let mut bytes = [0; 16];
            getrandom::fill(&mut bytes).unwrap();
            bytes[..8].copy_from_slice(&[0xfe, 0x80, 0, 0, 0, 0, 0, 0]);
            let local = IpAddress::Ipv6(bytes.into());
            (smoltcp::wire::IpCidr::new(local, 128),
                IpAddress::Ipv6(icmp_discovery::ALL_NODES_ADDRESS), fallback)
        } else { (cidr, parsed_peer, None) };
        let self_addr = cidr.address();
        let iface = Rc::new(RefCell::new(Interface::new(device, cidr)));
        if let Some((cidr, _)) = fallback {
            iface.borrow_mut().add_ip_address(cidr);
        }
        let iface_cpy = Rc::downgrade(&iface);

        // A public IPv6 listener says nothing about the inner tunnel family.
        // Only IPv4 can use a registered peer without discovery.
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
            if let Some((_, peer)) = fallback { discovery = discovery.with_ipv4_fallback(peer); }
            (Rc::new(RefCell::new(None)), Some(handle),
                Some(Rc::new(RefCell::new(discovery))))
        } else {
            (Rc::new(RefCell::new(Some(parsed_peer))), None, None)
        };

        let stream = TcpStream::new(iface.clone());

        let stream = Rc::new(RefCell::new(stream));

        let http = Rc::new(HttpRequests::new());
        let http_weak = Rc::downgrade(&http);
        let peer_weak = Rc::downgrade(&internal_peer_ip);
        let discovery_weak = discovery.as_ref().map(Rc::downgrade);
        let closure = Closure::<dyn FnMut(_)>::new(move |_: MessageEvent| {
            let Some(iface) = iface_cpy.upgrade() else { return; };
            iface.borrow_mut().poll();
            // Resume HTTP once the network stack has had a chance to make progress.
            if let Some(http) = http_weak.upgrade() {
                http.waker.wake();
            }
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
            http,
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

    async fn fetch(
        &self,
        request: web_sys::Request,
        url: &str,
        username: Option<&str>,
        password: Option<&str>,
    ) -> Result<Response, JsValue> {
        if self.http.disconnect.borrow().is_none() {
            return Err(js_sys::Error::new("Tunnel disconnected").into());
        }

        // Each request waits for its predecessor to finish before using the shared TCP socket.
        // Dropping the sender also releases the next request on errors.
        let (_finished, next) = oneshot::channel();
        let previous = self.http.tail.replace(Some(next));
        let request = async {
            if let Some(previous) = previous {
                let _ = previous.await;
            }
            self.send_http_request(request, url, username, password)
                .await
        };
        match select(self.http.disconnected.clone(), pin!(request)).await {
            Either::Left(_) => Err(js_sys::Error::new("Tunnel disconnected").into()),
            Either::Right((response, _)) => response,
        }
    }

    async fn send_http_request(
        &self,
        request: web_sys::Request,
        url: &str,
        username: Option<&str>,
        password: Option<&str>,
    ) -> Result<Response, JsValue> {
        if let Some(username) = username {
            *self.username.borrow_mut() = urlencoding::decode(username)
                .map_err(JsError::from)?
                .into_owned();
        }
        if let Some(password) = password {
            *self.password.borrow_mut() = urlencoding::decode(password)
                .map_err(JsError::from)?
                .into_owned();
        }

        let request = self.build_http_request(request, url).await?;
        let peer = poll_fn(|cx| {
            let stream = self.stream.borrow();
            if stream.is_up() && !stream.is_open() {
                if let Some(peer) = *self.internal_peer_ip.borrow() {
                    return Poll::Ready(peer);
                }
            }
            self.http.waker.register(cx.waker());
            Poll::Pending
        })
        .await;

        let local_port = 49152 + (js_sys::Math::random() * 16384.0) as u16;
        self.stream
            .borrow_mut()
            .connect(IpEndpoint::new(peer, self.port), local_port)
            .map_err(|err| js_sys::Error::new(&format!("TCP connect failed: {err}")))?;

        let response = self.exchange_http(request).await;
        {
            let mut stream = self.stream.borrow_mut();
            stream.close();
            stream.poll();
        }
        self.http.waker.take();
        let (parts, body) = response?;
        self.build_http_response(parts, body)
    }

    async fn exchange_http(
        &self,
        request: http::Request<Full<Bytes>>,
    ) -> Result<(http::response::Parts, Bytes), JsValue> {
        poll_fn(|cx| {
            let stream = self.stream.borrow();
            if stream.can_send() {
                Poll::Ready(Ok(()))
            } else if !stream.is_open() {
                Poll::Ready(Err(js_sys::Error::new(
                    "TCP connection closed before the request was sent",
                )))
            } else {
                self.http.waker.register(cx.waker());
                Poll::Pending
            }
        })
        .await?;

        let stream = HyperStream::new(self.stream.clone(), &self.http.waker);
        let (mut sender, connection) = hyper::client::conn::http1::handshake(stream)
            .await
            .map_err(JsError::from)?;
        let response = async move {
            let (parts, body) = sender.send_request(request).await?.into_parts();
            Ok::<_, hyper::Error>((parts, body.collect().await?.to_bytes()))
        };

        // Hyper's connection must run while the response headers and body are read.
        // Keep both futures here so disconnect can drop the entire exchange.
        match select(pin!(response), pin!(connection)).await {
            Either::Left((response, _)) => response.map_err(|err| JsError::from(err).into()),
            Either::Right((connection, response)) => {
                connection.map_err(JsError::from)?;
                Ok(response.await.map_err(JsError::from)?)
            }
        }
    }

    async fn build_http_request(
        &self,
        request: web_sys::Request,
        url: &str,
    ) -> Result<http::Request<Full<Bytes>>, JsValue> {
        let body = JsFuture::from(request.array_buffer()?).await?;
        let method = request.method();
        let mut builder = http::Request::builder().method(method.as_str());
        let headers = request.headers();
        for key in js_sys::Object::keys(&headers).iter() {
            if let Some(key) = key.as_string() {
                if let Some(value) = headers.get(&key)? {
                    builder = builder.header(key, value);
                }
            }
        }
        if !self.nonce.borrow().is_empty() {
            builder = builder.header(
                "Authorization",
                self.build_authentication_header(&method, url),
            );
        }

        let body = Full::new(Bytes::from(js_sys::Uint8Array::new(&body).to_vec()));
        Ok(builder
            .header("Content-Type", "application/json; charset=utf-8")
            .uri(url)
            .body(body)
            .map_err(JsError::from)?)
    }

    fn build_http_response(
        &self,
        parts: http::response::Parts,
        body: Bytes,
    ) -> Result<Response, JsValue> {
        let mut decoded = String::new();
        let body = match parts
            .headers
            .get("Content-Encoding")
            .map(|encoding| encoding.as_bytes())
        {
            Some(b"gzip") => {
                if let Err(err) = GzDecoder::new(body.as_ref()).read_to_string(&mut decoded) {
                    log::error!("Error decoding gzip: {err}");
                }
                decoded.as_bytes()
            }
            Some(b"deflate") => {
                if let Err(err) = ZlibDecoder::new(body.as_ref()).read_to_string(&mut decoded) {
                    log::error!("Error decoding deflate: {err}");
                }
                decoded.as_bytes()
            }
            Some(encoding) => {
                log::error!("Unknown encoding: {}", String::from_utf8_lossy(encoding));
                body.as_ref()
            }
            None => body.as_ref(),
        };

        let headers = web_sys::Headers::new()?;
        for (key, value) in &parts.headers {
            if let Ok(value) = value.to_str() {
                headers.set(key.as_str(), value)?;
            }
        }
        if let Some(challenge) = headers.get("www-authenticate")? {
            for param in challenge.split(',') {
                let Some((key, value)) = param.split_once('=') else {
                    continue;
                };
                let value = value.trim().trim_matches('"');
                match key.trim() {
                    "Digest realm" => *self.realm.borrow_mut() = value.to_owned(),
                    "nonce" => *self.nonce.borrow_mut() = value.to_owned(),
                    "opaque" => *self.opaque.borrow_mut() = value.to_owned(),
                    "qop" if value == "auth" => (),
                    "qop" => log::error!("Got qop {value}, which is not supported"),
                    key => log::info!("Got unknown value in www-authenticate header: {key}"),
                }
            }
        }

        let init = web_sys::ResponseInit::new();
        init.set_status(parts.status.as_u16());
        init.set_headers(&headers);
        init.set_status_text(parts.status.canonical_reason().unwrap_or(""));
        let array = match parts.status.as_u16() {
            204 | 205 | 304 => None,
            _ => Some(js_sys::Uint8Array::from(body)),
        };
        Response::new_with_opt_buffer_source_and_init(array.as_deref(), &init)
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

    #[wasm_bindgen_test(async)]
    async fn disconnect_and_free_cancel_pending_fetches_without_borrowing_the_exported_client() {
        let engine = base64::engine::general_purpose::STANDARD;
        for started in [false, true] {
            let noop = js_sys::Function::new_no_args("");
            let client: JsValue = Client::new(
                &engine.encode([1; 32]),
                &engine.encode(x25519::PublicKey::from(&x25519::StaticSecret::from([2; 32])).as_bytes()),
                &engine.encode([3; 32]), "ws://127.0.0.1:65534/lifetime",
                "10.123.1.3", "10.123.1.2", 80, 1420,
                noop.clone(), noop, Some(false), None, None,
            ).into();
            let fetch = js_sys::Reflect::get(&client, &"fetch".into()).unwrap()
                .unchecked_into::<js_sys::Function>();
            let mut requests = Vec::new();
            for _ in 0..3 {
                let request = web_sys::Request::new_with_str("http://test-peer/pending").unwrap();
                let promise = fetch.call2(&client, &request, &"/pending".into()).unwrap();
                requests.push(JsFuture::from(Promise::from(promise)));
            }
            if started { delay().await; }
            for method in ["disconnect", "free"] {
                js_sys::Reflect::get(&client, &method.into()).unwrap()
                    .unchecked_into::<js_sys::Function>().call0(&client).unwrap();
            }
            for request in requests {
                let error = request.await.expect_err("disconnected request stayed active");
                assert_eq!(js_sys::Error::from(error).message(), "Tunnel disconnected");
            }
        }
    }

    // Requires: cargo run --target x86_64-unknown-linux-gnu --example tunnel_peer
    async fn exercise(outer: &str, local: &str, peer: &str, discover: bool, fallback: Option<(&str, &str)>) {
        crate::set_pcap_logging(true);
        let expect_ipv6 = outer.contains("/v6");
        let mut previous_local = None;
        let mut previous_peer = None;
        // Recreate the complete client to cover reconnection with new WG sessions.
        for _ in 0..2 {
            let noop = Closure::<dyn FnMut()>::new(|| {});
            let engine = base64::engine::general_purpose::STANDARD;
            let client = Client::new(
                &engine.encode([1; 32]),
                &engine.encode(x25519::PublicKey::from(&x25519::StaticSecret::from([2; 32])).as_bytes()),
                &engine.encode([3; 32]), outer, local, peer, 80, 1420,
                noop.as_ref().unchecked_ref::<js_sys::Function>().clone(),
                noop.as_ref().unchecked_ref::<js_sys::Function>().clone(), Some(discover),
                fallback.map(|(local, _)| local.to_owned()), fallback.map(|(_, peer)| peer.to_owned()),
            );
            let discovery_required = discover || peer.contains(':');
            assert_eq!(client.peer_discovery_complete(), !discovery_required);
            wait_for(|| client.0.iface.borrow().is_up() && client.peer_discovery_complete()).await;
            if discovery_required && expect_ipv6 {
                let discovered = client.0.internal_peer_ip.borrow().unwrap();
                let IpAddress::Ipv6(address) = discovered else { panic!("discovered IPv4 peer"); };
                assert!(address.is_unicast_link_local());
                assert_ne!(discovered, icmp_discovery::parse_cidr_address(peer).unwrap(),
                    "saved IPv6 peer was used instead of discovery");
                assert_ne!(previous_peer, Some(discovered));
                previous_peer = Some(discovered);
            }
            let request = web_sys::Request::new_with_str("http://test-peer/status").unwrap();
            let response = Response::from(JsFuture::from(client.fetch(request, "/status".into(), None, None)).await.unwrap());
            assert_eq!(response.status(), 200);
            let body = JsFuture::from(response.text().unwrap()).await.unwrap().as_string().unwrap();
            assert_eq!(body, if expect_ipv6 { "peer-v6" } else { "peer-v4" });
            let messages = Rc::new(RefCell::new(Vec::new()));
            let received = messages.clone();
            let callback = Closure::<dyn FnMut(String)>::new(move |message| received.borrow_mut().push(message));
            client.start_inner_ws(callback.as_ref().unchecked_ref::<js_sys::Function>().clone());
            wait_for(|| !messages.borrow().is_empty()).await;
            assert_eq!(&*messages.borrow(), &["peer-ws"]);
            let source = assert_capture(&client, fallback.filter(|_| !expect_ipv6).map(|p| p.0).unwrap_or(local), discovery_required, expect_ipv6);
            if discovery_required {
                assert_ne!(previous_local, source, "replacement reused the IPv6 probe source");
                assert_ne!(source.map(IpAddress::Ipv6), Some(icmp_discovery::parse_cidr_address(local).unwrap()),
                    "saved address was used as the IPv6 probe source");
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
                            assert_eq!(
                                &ip.src_addr().octets()[..8],
                                &[0xfe, 0x80, 0, 0, 0, 0, 0, 0]
                            );
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
    async fn saved_ipv6_addresses_with_64_prefix_require_discovery() {
        exercise("ws://[::1]:18081/v6", "fd00::3/64", "fd00::2/64", false, None).await;
    }

    #[wasm_bindgen_test(async)]
    async fn saved_ipv6_addresses_with_128_prefix_require_discovery() {
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
        exercise("ws://127.0.0.1:18081/v4", "fd00::3/128", "ff02::1", true,
            Some(("10.123.1.3/24", "10.123.1.2/32"))).await;
    }
}
