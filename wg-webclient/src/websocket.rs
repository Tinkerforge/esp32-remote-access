use std::{cell::RefCell, io::Write, rc::Rc};

use tungstenite::handshake::client::{generate_key, generate_request, Request};
use wasm_bindgen::{closure::Closure, JsValue};
use crate::console_log;
use crate::stream::TcpStream;


pub struct Websocket {
}

impl Websocket {
    pub fn connect<'a, Device>(stream: &mut TcpStream<'a, Device>)
     where Device: smoltcp::phy::Device {
        let key = generate_key();
        let request = Request::builder()
            .method("GET")
            .uri("/ws")
            .header("Sec-WebSocket-Key", key)
            .header("Upgrade", "websocket")
            .header("Connection", "Upgrade")
            .header("Sec-WebSocket-Version", "13")
            .header("Host", "bla")
            .body(()).unwrap();
        let (request, _) = match generate_request(request) {
            Ok(req) => req,
            Err(e) => {
                console_log!("error: {}", e);
                return
            }
        };
        let len = stream.write(&request[..]).unwrap();
        stream.flush().unwrap();
        match std::str::from_utf8(&request[..]) {
            Ok(req) => console_log!("request: {}", req),
            Err(e) => console_log!("error while decoding request: {}", e.to_string())
        }
        console_log!("written {} bytes", len);
     }
}
