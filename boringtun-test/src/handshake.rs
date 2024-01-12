use std::{io::{Read, Write}, collections::HashMap};

use anyhow::Result;
use http::request;
use tungstenite::handshake::client::generate_key;

pub struct RequestWrap<T>(http::Request<T>);

impl<T> RequestWrap<T>
where T: IntoIterator<Item=u8> {
    pub fn new(request: http::Request<T>) -> Self {
        Self(request)
    }

    pub fn build(self) -> Vec<u8> {
        let mut buf = Vec::new();
        let (parts, body) = self.0.into_parts();
        buf.extend(format!("{} {} {}\r\n", parts.method, parts.uri, "HTTP/1.1").as_bytes());
        for (key, value) in parts.headers.into_iter() {
            buf.extend(format!("{}: {}\r\n", key.unwrap(), value.to_str().unwrap()).as_bytes());
        }
        buf.extend("\r\n".as_bytes());
        buf.extend(body);
        buf
    }
}

impl RequestWrap<()>  {
    pub fn new_get(request: http::Request<()>) -> Self {
        Self(request)
    }

    pub fn build_get(self) -> Vec<u8> {
        let mut buf = Vec::new();
        let (parts, _) = self.0.into_parts();
        buf.extend(format!("{} {} {}\r\n", parts.method, parts.uri, "HTTP/1.1").as_bytes());
        for (key, value) in parts.headers.into_iter() {
            buf.extend(format!("{}: {}\r\n", key.unwrap(), value.to_str().unwrap()).as_bytes());
        }
        buf.extend("\r\n".as_bytes());
        buf
    }
}

struct ResponseWrap<T>(http::Response<T>);

impl<T> ResponseWrap<T>
where T: IntoIterator<Item=u8> {
    pub fn new(response: http::Response<T>) -> Self {
        Self(response)
    }

    pub fn build(self) -> Vec<u8> {
        let mut buf = Vec::new();
        let (parts, body) = self.0.into_parts();
        buf.extend(format!("{} {}\r\n", "HTTP/1.1", parts.status).as_bytes());
        for (key, value) in parts.headers.into_iter() {
            buf.extend(format!("{}: {}\r\n", key.unwrap(), value.to_str().unwrap()).as_bytes());
        }
        buf.extend("\r\n".as_bytes());
        buf.extend(body);
        buf
    }
}

impl ResponseWrap<()> {
    pub fn new_no_body(response: http::Response<()>) -> Self {
        Self(response)
    }

    pub fn build_no_body(self) -> Vec<u8> {
        let mut buf = Vec::new();
        let (parts, _) = self.0.into_parts();
        buf.extend(format!("{} {}\r\n", "HTTP/1.1", parts.status).as_bytes());
        for (key, value) in parts.headers.into_iter() {
            buf.extend(format!("{}: {}\r\n", key.unwrap(), value.to_str().unwrap()).as_bytes());
        }
        buf.extend("\r\n".as_bytes());
        buf
    }
}


struct Handshake<Stream>(Stream)
where Stream: Read + Write;

impl <Stream> Handshake<Stream>
where Stream: Read + Write {
    pub fn new(stream: Stream) -> Self {
        Self(stream)
    }

    pub fn start_client(&mut self, url: &str) -> Result<()> {
        let request = http::Request::builder()
            .uri(url.to_string())
            .method("GET")
            .header("Upgrade", "websocket")
            .header("Connection", "Upgrade")
            .header("Sec-WebSocket-Key", generate_key())
            .header("Sec-WebSocket-Version", "13")
            .body(());

        let request = RequestWrap::new_get(request.unwrap());
        self.0.write(&request.build_get())?;
        self.0.flush()?;
        Ok(())
    }
}


#[cfg(test)]
pub mod test {
    use wasm_bindgen_test::*;

    use crate::stream::test::create_connected_stream_pair;

    #[wasm_bindgen_test]
    fn test_websocket_connection() {
        let (stream, stream2) = create_connected_stream_pair();

        let mut server = tungstenite::WebSocket::from_raw_socket(
            stream,
            tungstenite::protocol::Role::Server,
            None);

        let mut client = tungstenite::WebSocket::from_raw_socket(
            stream2,
            tungstenite::protocol::Role::Client,
            None);

        server.send(tungstenite::protocol::Message::Text("Hello world!".to_string())).unwrap();
        let msg = client.read().unwrap();
        let msg = msg.into_text().unwrap();
        assert_eq!(msg, "Hello world!");
    }
}
