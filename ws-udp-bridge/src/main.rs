use std::{fs::File, io::Read, sync::{Arc, Mutex}, vec};

use futures::{StreamExt, TryStreamExt};
use futures_channel::mpsc::UnboundedSender;
use futures_util::{future::{self}, pin_mut, SinkExt};
use tokio_native_tls::{native_tls::Identity, TlsAcceptor};
use tokio::net::{TcpListener, UdpSocket};
use tokio_tungstenite::tungstenite::protocol::Message;
use log::{debug, error, info, warn};
use simplelog::*;

type Tx = UnboundedSender<Vec<u8>>;
type Peer = Arc<Mutex<Option<Tx>>>;


#[tokio::main]
async fn main() -> anyhow::Result<()> {
    CombinedLogger::init(
        vec![TermLogger::new(LevelFilter::Debug, Config::default(), TerminalMode::Mixed, ColorChoice::Auto)]
    ).unwrap();

    info!("Starting");

    let udp_sock = UdpSocket::bind("0.0.0.0:51820").await?;
    let udp_sock = Arc::new(udp_sock);
    udp_sock.connect("192.168.1.123:51820").await?;
    let peer: Peer = Arc::new(Mutex::new(None));
    create_ws_server(peer.clone(), udp_sock.clone()).await?;

    let mut buf = [0u8; 2000];
    loop {
        loop {
            match udp_sock.recv(&mut buf).await {
                Ok(read) => {
                    if let Some(peer) = peer.lock().unwrap().as_mut() {
                        let _ = peer.send(buf[0..read].to_vec()).await;
                    }
                },
                Err(_) => break,
            }
        }
    }

}

async fn create_ws_server(peer: Peer, udp_sock: Arc<UdpSocket>) -> anyhow::Result<()> {
    info!("Creating Websocket Server");
    let (cert, key) = get_certificates()?;
    let identity = Identity::from_pkcs8(&cert, &key)?;

    let acceptor = tokio_native_tls::native_tls::TlsAcceptor::new(identity)?;
    let acceptor = TlsAcceptor::from(acceptor);
    let listener = TcpListener::bind("0.0.0.0:8081").await?;
    tokio::spawn(async move {
        while let Ok((stream, addr)) = listener.accept().await {
            let tls_stream = match acceptor.accept(stream).await {
                Ok(stream) => stream,
                Err(err) => {
                    error!("Failed to create tls stream: {}", err);
                    continue;
                }
            };

            let ws_stream = match tokio_tungstenite::accept_async(tls_stream).await {
                Ok(stream) =>  stream,
                Err(err) => {
                    error!("Failed to create websocket stream: {}", err);
                    continue;
                }
            };

            let udp_sock = udp_sock.clone();
            let peer = peer.clone();
            tokio::spawn(async move {
                let (this_tx, rx) = futures_channel::mpsc::unbounded();
                *peer.lock().unwrap() = Some(this_tx);

                let (outgoing, incoming) = ws_stream.split();
                let outgoing = Arc::new(Mutex::new(outgoing));
                let outgoing_cpy = outgoing.clone();
                let incoming = incoming.try_for_each(|msg| {
                    match msg {
                        Message::Close(_) => {
                            info!("Client {} disconnected", addr);
                            return futures_util::future::ok(())
                        },
                        Message::Ping(data) => {
                            let mut outgoing = outgoing_cpy.lock().unwrap();
                            let _ = futures::executor::block_on(outgoing.send(Message::Pong(data)));
                        },
                        Message::Binary(data) => {
                            match futures::executor::block_on(udp_sock.send(&data)) {
                                Ok(_) => (),
                                Err(err)  => debug!("Failed to send to udp: {}", err)
                            }
                        },
                        _ => warn!("Unknown ws message received")
                    }

                    futures_util::future::ok(())
                });

                let udp_recv = rx.for_each(|data| {
                    let mut outgoing = outgoing.lock().unwrap();
                    match futures::executor::block_on(outgoing.send(Message::Binary(data))) {
                        Ok(()) => (),
                        Err(err) => error!("Error while sending data to ws: {}", err)
                    }
                    future::ready(())
                });

                pin_mut!(incoming, udp_recv);
                future::select(incoming, udp_recv).await;
            });
        }
    });

    Ok(())
}

fn get_certificates() -> anyhow::Result<(Vec<u8>, Vec<u8>)> {
    let mut cert_file = File::open("/home/freddy/tf/esp32-remote-access/docker/nginx/certs/cert.pem")?;
    let mut key_file = File::open("/home/freddy/tf/esp32-remote-access/docker/nginx/certs/key.pem")?;

    let mut cert = vec![];
    let mut key = vec![];
    cert_file.read_to_end(&mut cert)?;
    key_file.read_to_end(&mut key)?;

    Ok((cert, key))
}
