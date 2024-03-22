
use std::io::Read;
use std::{cell::RefCell, io::Write, rc::Rc};

use tungstenite::handshake::client::{generate_key, generate_request, Request};
use tungstenite::handshake::derive_accept_key;
use tungstenite::handshake::machine::TryParse;
use wasm_bindgen::{closure::Closure, JsValue};
use crate::console_log;
use crate::interval_handle::IntervalHandle;
use crate::stream::TcpStream;
use crate::wg_device::IsUp;

struct ConnectingStruct<Device>
 where Device: smoltcp::phy::Device + Clone + IsUp {
    stream: TcpStream<'static, Device>,
    _interval: IntervalHandle<JsValue>,
}

struct ConnectedStruct<Device>
 where Device: smoltcp::phy::Device + Clone + IsUp {
    stream: Rc<RefCell<tungstenite::WebSocket<TcpStream<'static, Device>>>>,
    _interval: IntervalHandle<JsValue>,
}

enum WebsocketState<Device>
 where Device: smoltcp::phy::Device + Clone + IsUp {
    Created,
    Connecting(ConnectingStruct<Device>),
    Connected(ConnectedStruct<Device>),
    Disconnected,
}

/**
    This struct manages the complete inner Websocket.
 */
pub struct Websocket<Device>
 where Device: smoltcp::phy::Device + Clone + IsUp {
    state: Rc<RefCell<WebsocketState<Device>>>,
    cb: Rc<RefCell<Option<js_sys::Function>>>,
}

impl<Device> Websocket<Device>
 where Device: smoltcp::phy::Device + 'static + Clone + IsUp {
    /**
        Consumes the provided TcpStream, does the Websocket handshake and returns a Connected Websocket struct.
     */
    pub fn connect(mut stream: TcpStream<'static, Device>) -> Result<Self, tungstenite::Error> {
        let key = generate_key();
        let request = Request::builder()
            .method("GET")
            .uri("/ws")
            .header("Sec-WebSocket-Key", key)
            .header("Upgrade", "websocket")
            .header("Connection", "Upgrade")
            .header("Sec-WebSocket-Version", "13")
            .header("Host", "")
            .body(()).unwrap();
        let (request, key) = generate_request(request)?;
        let _ = stream.write(&request[..]).unwrap();

        let key = Rc::new(key);
        let key_cpy = key.clone();

        let state = Rc::new(RefCell::new(WebsocketState::Created));
        let state_cpy = state.clone();

        let cb = Rc::new(RefCell::new(None::<js_sys::Function>));
        let cb_cpy = cb.clone();

        let closure = Closure::<dyn FnMut(JsValue)>::wrap(Box::new(move |_: JsValue| {
            let mut state = state_cpy.borrow_mut();
            let mut stream = match &mut *state {
                WebsocketState::Connecting(connecting) => {
                    if !connecting.stream.can_send() {
                        return
                    }
                    let _ = connecting.stream.flush();
                    connecting.stream.clone()
                },
                _ => return
            };
            if !stream.can_recv() {
                return
            }
            let mut buf = [0u8; 4096];
            let len = match stream.read(&mut buf) {
                Ok(len) => len,
                Err(e) => {
                    console_log!("error while reading from stream: {}", e.to_string());
                    return
                }
            };

            let (_, response) = match tungstenite::handshake::client::Response::try_parse(&buf[..len]) {
                Ok(Some(response)) => response,
                Ok(None) => {
                    *state_cpy.borrow_mut() = WebsocketState::Disconnected;
                    return
                },
                Err(e) => {
                    console_log!("error while parsing response: {}", e.to_string());
                    return
                }
            };

            if let Some(accept_key) = response.headers().get("Sec-WebSocket-Accept") {
                if accept_key.as_bytes() != derive_accept_key(&key_cpy.as_bytes()).as_bytes() {
                    console_log!("invalid accept key");
                    return
                }
            } else {
                console_log!("no accept key");
                return
            }

            let cb_cpy = cb_cpy.clone();
            let state_cpy = state_cpy.clone();
            let closure = Closure::<dyn FnMut(_)>::new(move |_: JsValue| {
                let mut state = state_cpy.borrow_mut();
                let message = {
                    let mut stream = match *state {
                        WebsocketState::Connected(ref mut connected) => connected.stream.borrow_mut(),
                        _ => return
                    };
                    if !stream.can_read() {
                        return
                    }
                    match stream.read() {
                        Ok(msg) => msg,
                        Err(e) => {
                            match e {
                                tungstenite::Error::Io(_) => (),
                                _ => console_log!("error while reading from stream: {}", e.to_string())
                            }
                            return
                        }
                    }
                };

                match message {
                    tungstenite::Message::Text(text) => {
                        let cb = cb_cpy.borrow_mut();
                        if let Some(cb) = cb.as_ref() {
                            let this = JsValue::null();
                            let _ = cb.call1(&this, &JsValue::from_str(&text));
                        }
                    },
                    tungstenite::Message::Ping(_) => (),
                    _ => console_log!("unhandled message: {:?}", message)
                }
            });

            *state = WebsocketState::Connected(ConnectedStruct {
                stream: Rc::new(RefCell::new(tungstenite::WebSocket::from_raw_socket(stream, tungstenite::protocol::Role::Client, None))),
                _interval: IntervalHandle::new(closure, 0),
            });
        }));

        let interval = IntervalHandle::new(closure, 0);
        *state.borrow_mut() = WebsocketState::Connecting(ConnectingStruct {
            stream,
            _interval: interval,
        });

        Ok(Self {
            state,
            cb,
        })
     }

     pub fn disconnect(&self) {
        match &*self.state.borrow_mut() {
            WebsocketState::Connected(state) => {
                let mut socket = state.stream.borrow_mut();
                let _ = socket.close(None);
                let _ = socket.flush();
            },
            _ => ()
        }
     }

     /**
        Sets the onMessage callback.
      */
     pub fn on_message(&mut self, cb: js_sys::Function) {
        *self.cb.borrow_mut() = Some(cb);
     }
}
