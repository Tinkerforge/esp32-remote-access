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
    io::{self, Read, Write},
};

use tungstenite::{
    handshake::{
        client::{generate_key, generate_request, Request, Response},
        derive_accept_key,
        machine::TryParse,
    },
    protocol::Role,
    Error, Message,
};
use wasm_bindgen::JsValue;

use crate::{stream::TcpStream, wg_device::IsUp};

struct Connecting<Device: smoltcp::phy::Device + Clone + IsUp + 'static> {
    stream: TcpStream<'static, Device>,
    request: Vec<u8>,
    accept_key: String,
    response: Vec<u8>,
}

impl<Device: smoltcp::phy::Device + Clone + IsUp> Connecting<Device> {
    fn poll(
        &mut self,
    ) -> Result<Option<tungstenite::WebSocket<TcpStream<'static, Device>>>, Error> {
        if !self.stream.is_open() {
            return Err(Error::ConnectionClosed);
        }
        if !self.request.is_empty() && self.stream.can_send() {
            self.stream.write_all(&self.request)?;
            self.stream.flush()?;
            self.request.clear();
        }

        while self.stream.can_recv() {
            let mut buf = [0; 4096];
            let len = self.stream.read(&mut buf)?;
            self.response.extend_from_slice(&buf[..len]);
            if let Some((end, response)) = Response::try_parse(&self.response)? {
                if response
                    .headers()
                    .get("Sec-WebSocket-Accept")
                    .is_none_or(|key| key.as_bytes() != self.accept_key.as_bytes())
                {
                    return Err(Error::Protocol(
                        tungstenite::error::ProtocolError::SecWebSocketAcceptKeyMismatch,
                    ));
                }
                return Ok(Some(tungstenite::WebSocket::from_partially_read(
                    self.stream.clone(),
                    self.response.split_off(end),
                    Role::Client,
                    None,
                )));
            }
            if self.response.len() >= buf.len() {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "WebSocket upgrade headers are too large",
                )
                .into());
            }
        }
        Ok(None)
    }
}

enum State<Device: smoltcp::phy::Device + Clone + IsUp + 'static> {
    Connecting(Connecting<Device>),
    Connected(tungstenite::WebSocket<TcpStream<'static, Device>>),
    Disconnected,
}

/// Inner WebSocket, progressed after the tunnel's TCP stack is polled.
pub struct Websocket<Device: smoltcp::phy::Device + Clone + IsUp + 'static> {
    state: RefCell<State<Device>>,
    on_message: js_sys::Function,
}

impl<Device: smoltcp::phy::Device + Clone + IsUp> Websocket<Device> {
    pub fn connect(
        stream: TcpStream<'static, Device>,
        auth_header: Option<String>,
        on_message: js_sys::Function,
    ) -> Result<Self, Error> {
        let mut request = Request::builder()
            .method("GET")
            .uri("/ws")
            .header("Sec-WebSocket-Key", generate_key())
            .header("Upgrade", "websocket")
            .header("Connection", "Upgrade")
            .header("Sec-WebSocket-Version", "13")
            .header("Host", "");

        if let Some(auth_header) = auth_header {
            request = request.header("Authorization", auth_header);
        }

        let (request, key) = generate_request(request.body(())?)?;
        Ok(Self {
            state: RefCell::new(State::Connecting(Connecting {
                stream,
                request,
                accept_key: derive_accept_key(key.as_bytes()),
                response: Vec::new(),
            })),
            on_message,
        })
    }

    pub fn poll(&self) {
        loop {
            let message = {
                let mut state = self.state.borrow_mut();
                if let State::Connecting(connecting) = &mut *state {
                    match connecting.poll() {
                        Ok(Some(socket)) => *state = State::Connected(socket),
                        Ok(None) => return,
                        Err(err) => {
                            log::debug!("Inner WebSocket upgrade ended: {err}");
                            *state = State::Disconnected;
                            return;
                        }
                    }
                }
                let State::Connected(socket) = &mut *state else {
                    return;
                };
                if !socket.can_read() {
                    return;
                }
                match socket.read() {
                    Ok(Message::Close(frame)) => {
                        log::info!("WebSocket closed: {frame:?}");
                        let _ = socket.flush();
                        *state = State::Disconnected;
                        return;
                    }
                    Ok(message) => message,
                    Err(Error::Io(err))
                        if matches!(
                            err.kind(),
                            io::ErrorKind::WouldBlock | io::ErrorKind::BrokenPipe
                        ) =>
                    {
                        return
                    }
                    Err(err) => {
                        log::error!("Error while reading from inner WebSocket: {err}");
                        *state = State::Disconnected;
                        return;
                    }
                }
            };
            // A JavaScript callback can disconnect or replace this socket.
            match message {
                Message::Text(text) => {
                    let _ = self
                        .on_message
                        .call1(&JsValue::NULL, &JsValue::from_str(&text));
                }
                Message::Ping(_) => (),
                _ => log::error!("Unhandled WebSocket message: {message:?}"),
            }
        }
    }

    pub fn disconnect(&self) {
        let mut state = self.state.borrow_mut();
        if let State::Connected(socket) = &mut *state {
            let _ = socket.close(None);
            let _ = socket.flush();
        }
        *state = State::Disconnected;
    }
}

impl<Device: smoltcp::phy::Device + Clone + IsUp> Drop for Websocket<Device> {
    fn drop(&mut self) {
        self.disconnect();
    }
}
