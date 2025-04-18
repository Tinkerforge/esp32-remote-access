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

use std::io::Read;
use std::rc::Weak;
use std::{cell::RefCell, io::Write, rc::Rc};

use crate::interval_handle::IntervalHandle;
use crate::stream::TcpStream;
use crate::wg_device::IsUp;
use tungstenite::handshake::client::{generate_key, generate_request, Request};
use tungstenite::handshake::derive_accept_key;
use tungstenite::handshake::machine::TryParse;
use wasm_bindgen::{closure::Closure, JsValue};

struct ConnectingStruct<Device>
where
    Device: smoltcp::phy::Device + Clone + IsUp,
{
    stream: TcpStream<'static, Device>,
    _interval: IntervalHandle<JsValue>,
}

struct ConnectedStruct<Device>
where
    Device: smoltcp::phy::Device + Clone + IsUp,
{
    stream: Rc<RefCell<tungstenite::WebSocket<TcpStream<'static, Device>>>>,
    _interval: IntervalHandle<JsValue>,
}

enum WebsocketState<Device>
where
    Device: smoltcp::phy::Device + Clone + IsUp,
{
    Created,
    Connecting(ConnectingStruct<Device>),
    Connected(ConnectedStruct<Device>),
    Disconnected,
}

/**
   This struct manages the complete inner Websocket.
*/
pub struct Websocket<Device>
where
    Device: smoltcp::phy::Device + 'static + Clone + IsUp,
{
    state: Rc<RefCell<WebsocketState<Device>>>,
    cb: Rc<RefCell<Option<js_sys::Function>>>,
}

impl<Device> Websocket<Device>
where
    Device: smoltcp::phy::Device + 'static + Clone + IsUp,
{
    /**
       Consumes the provided TcpStream, does the Websocket handshake and returns a Connected Websocket struct.
    */
    pub fn connect(mut stream: TcpStream<'static, Device>, auth_header: Option<String>) -> Result<Self, tungstenite::Error> {
        let key = generate_key();
        let mut request = Request::builder()
            .method("GET")
            .uri("/ws")
            .header("Sec-WebSocket-Key", key)
            .header("Upgrade", "websocket")
            .header("Connection", "Upgrade")
            .header("Sec-WebSocket-Version", "13")
            .header("Host", "");

        if let Some(auth_header) = auth_header {
            request = request.header("Authorization", auth_header);
        }

        let request = request.body(())?;
        let (request, key) = generate_request(request)?;
        let _ = stream.write(&request[..])?;

        let key = Rc::new(key);
        let state = Rc::new(RefCell::new(WebsocketState::Created));
        let cb = Rc::new(RefCell::new(None::<js_sys::Function>));
        let setup_closure = create_setup_closure(Rc::downgrade(&state), key, Rc::downgrade(&cb));

        let interval = IntervalHandle::new(setup_closure, 0);
        *state.borrow_mut() = WebsocketState::Connecting(ConnectingStruct {
            stream,
            _interval: interval,
        });

        Ok(Self { state, cb })
    }

    pub fn disconnect(&self) {
        match &*self.state.borrow_mut() {
            WebsocketState::Connected(state) => {
                let mut socket = state.stream.borrow_mut();
                let _ = socket.close(None);
                let _ = socket.flush();
            }
            _ => (),
        }
    }

    /**
      Sets the onMessage callback.
    */
    pub fn on_message(&mut self, cb: js_sys::Function) {
        *self.cb.borrow_mut() = Some(cb);
    }
}

impl<Device> Drop for Websocket<Device>
where
    Device: smoltcp::phy::Device + 'static + Clone + IsUp,
{
    fn drop(&mut self) {
        self.disconnect();
    }
}

/**
 * Creates a closure that sets up the Websocket connection through
 * the etablished WireGuard tunnel.
 */
fn create_setup_closure<Device>(
    state: Weak<RefCell<WebsocketState<Device>>>,
    key: Rc<String>,
    cb: Weak<RefCell<Option<js_sys::Function>>>,)
    -> Closure<dyn FnMut(JsValue)>
    where Device: smoltcp::phy::Device + Clone + IsUp + 'static
{
    Closure::<dyn FnMut(JsValue)>::wrap(Box::new(move |_: JsValue| {
        let state_rc = state.upgrade().unwrap();
        let mut state_ref = state_rc.borrow_mut();

        // In case the Websocket is already connected, we don't need to do anything.
        let mut stream = match &mut *state_ref {
            WebsocketState::Connecting(connecting) => {
                if !connecting.stream.can_send() {
                    return;
                }
                let _ = connecting.stream.flush();
                connecting.stream.clone()
            }
            _ => return,
        };
        if !stream.can_recv() {
            return;
        }
        let mut buf = [0u8; 4096];
        let read: usize = match stream.read(&mut buf) {
            Ok(len) => len,
            Err(e) => {
                log::error!("error while reading from stream: {}", e.to_string());
                return;
            }
        };
        log::trace!("read len {}", read);
        let (cursor, response) =
            match tungstenite::handshake::client::Response::try_parse(&buf[..read]) {
                Ok(Some(response)) => response,
                Ok(None) => {
                    *state_ref = WebsocketState::Disconnected;
                    return;
                }
                Err(e) => {
                    log::error!("error while parsing response: {}", e.to_string());
                    return;
                }
            };

        if let Some(accept_key) = response.headers().get("Sec-WebSocket-Accept") {
            if accept_key.as_bytes() != derive_accept_key(&key.as_bytes()).as_bytes() {
                panic!("invalid accept key");
            }
        } else {
            panic!("no accept key");
        }

        let closure = create_websocket_closure(state.clone(), cb.clone());

        let interval = IntervalHandle::new(closure, 0);
        let ws = if cursor == read {
            tungstenite::WebSocket::from_raw_socket(
                stream,
                tungstenite::protocol::Role::Client,
                None,
            )
        } else {
            tungstenite::WebSocket::from_partially_read(stream, buf[cursor..read].to_vec(), tungstenite::protocol::Role::Client, None)
        };
        *state_ref = WebsocketState::Connected(ConnectedStruct {
            stream: Rc::new(RefCell::new(ws)),
            _interval: interval,
        });
    }))
}

/**
 * Creates the closure that handles the actual Websocket connection
 */
fn create_websocket_closure<Device>(
    state: Weak<RefCell<WebsocketState<Device>>>,
    cb: Weak<RefCell<Option<js_sys::Function>>>,
) -> Closure<dyn FnMut(JsValue)>
    where Device: smoltcp::phy::Device + Clone + IsUp + 'static {
        Closure::<dyn FnMut(_)>::new(move |_: JsValue| {
        let state = state.upgrade().unwrap();
        let mut state = state.borrow_mut();
        let message = {
            let mut socket = match *state {
                WebsocketState::Connected(ref mut connected) => {
                    connected.stream.borrow_mut()
                }
                _ => return,
            };
            if !socket.can_read() {
                return;
            }
            match socket.read() {
                Ok(msg) => msg,
                Err(e) => {
                    match e {
                        tungstenite::Error::Io(err) => {
                            match err.kind() {
                                std::io::ErrorKind::WouldBlock => (),

                                // This error happens when updating a firmware and when rebooting the charger.
                                // The underlying tcp stream should be able to recover
                                std::io::ErrorKind::BrokenPipe => (),
                                err => {
                                    log::error!("Error: {err:?}");
                                }
                            }
                        },
                        _ => log::error!(
                            "error while reading from Websocket: {}",
                            e.to_string()
                        ),
                    }
                    return;
                }
            }
        };

        match message {
            tungstenite::Message::Text(text) => {
                let cb = cb.upgrade().unwrap();
                let cb = cb.borrow_mut();
                if let Some(cb) = cb.as_ref() {
                    let this = JsValue::null();
                    let _ = cb.call1(&this, &JsValue::from_str(&text));
                }
            }
            tungstenite::Message::Ping(_) => (),
            _ => log::error!("unhandled message: {:?}", message),
        }
    })
}
