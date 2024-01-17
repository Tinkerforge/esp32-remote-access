use std::{sync::{Arc, Mutex},  thread, time::{Instant, SystemTime}};

use websocket::{sync::Server, OwnedMessage};
use tokio::net::UdpSocket;

fn log(s: &str) {
    println!("{:?}: {}", Instant::now(), s);
}

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let (ws_tx, mut main_rx) = tokio::sync::mpsc::channel::<Vec<u8>>(32);
    let (main_tx, ws_rx) = std::sync::mpsc::channel::<Vec<u8>>();

    // let ws_tx = Arc::new(Mutex::new(ws_tx));
    let ws_rx = Arc::new(Mutex::new(ws_rx));
    // let main_tx = Arc::new(Mutex::new(main_tx));
    // let main_rx = Arc::new(Mutex::new(main_rx));

    // let timer = Arc::new(Mutex::new(Instant::now()));
    let main_tx_cpy = main_tx.clone();
    thread::spawn(move || {
        let server = Server::bind("0.0.0.0:8081").unwrap();
        for request in server.filter_map(Result::ok) {
            let ws_tx = ws_tx.clone();
            let ws_rx = ws_rx.clone();
            let main_tx = main_tx_cpy.clone();
            thread::spawn(move || {
                let client = request.accept().unwrap();
                let ip = client.peer_addr().unwrap();
                println!("Connection from {}", ip);
                let (mut receiver, sender) = client.split().unwrap();

                let sender = Arc::new(Mutex::new(sender));
                let sender_cpy = sender.clone();
                let shutdown = Arc::new(Mutex::new(false));
                let shutdown_cpy = shutdown.clone();
                thread::spawn(move || {
                    loop {
                        let message = ws_rx.lock().unwrap().recv();
                        if *shutdown_cpy.lock().unwrap() {
                            return;
                        }
                        // println!("Sending message to {}: {:?}", ip, message);
                        log("Message to web");
                        sender_cpy.lock().unwrap().send_message(&OwnedMessage::Binary(message.unwrap())).unwrap();
                    }
                });

                loop {
                    // println!("Waiting for message from {}", ip);
                    let message = receiver.recv_message().unwrap();
                    match message {
                        OwnedMessage::Binary(message) => {
                            futures::executor::block_on(ws_tx.send(message)).unwrap();
                            println!("Message to udp")
                        },
                        OwnedMessage::Close(_) => {
                            let _ = sender.lock().unwrap().send_message(&OwnedMessage::Close(None));
                            println!("Client {} disconnected", ip);
                            *shutdown.lock().unwrap() = true;
                            main_tx.send(vec![]).unwrap();
                            return;
                        },
                        _ => println!("Unhandled message from {}: {:?}", ip, message),
                    }
                }
            });
        }
    });


    let udp_sock = UdpSocket::bind("0.0.0.0:51820").await?;

    let remote_addr = "192.168.1.123:51820";
    udp_sock.connect(remote_addr).await?;
    let mut buf = [0u8; 2048];
    println!("Entering udp loop");
    loop {
        tokio::select! {
            Ok(len) = udp_sock.recv(&mut buf) => {
                main_tx.send(buf[..len].to_vec()).unwrap();
            },
            Some(message) = main_rx.recv() => {
                // println!("Sending message to {}: {:?}", remote_addr, message);
                log("Message to udp");
                udp_sock.send(&message).await.unwrap();
            },
            else => panic!("Something went wrong"),
        }
    }
}
