mod http;
mod util;

use boringtun::noise::TunnResult;
use clap::Parser;
use futures_util::{SinkExt, TryStreamExt};
use reqwest_websocket::Message;
use std::ffi::CString;

use http::Cache;

unsafe extern "C" {
    fn tun_alloc(dev: *const std::os::raw::c_char, self_ip: *const std::os::raw::c_char, peer_ip: *const std::os::raw::c_char) -> i32;
}

#[derive(Parser, Debug)]
struct Cli {
    #[arg(long, env = "HOST")]
    host: Option<String>,
    #[arg(short, long, env = "EMAIL")]
    email: String,
    #[arg(short, long, env = "PASSWORD")]
    password: String,

    device: uuid::Uuid,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();

    simplelog::TermLogger::init(
        simplelog::LevelFilter::Debug,
        simplelog::Config::default(),
        simplelog::TerminalMode::Mixed,
        simplelog::ColorChoice::Auto,
    )?;

    let args = Cli::parse();

    let cache = Cache {
        cookie: String::new(),
        host: "tf-freddy".to_string(),
        secret: String::new(),
    };
    let mut client = http::Client::new(cache, true)?;
    client.login(args.email, &args.password).await?;
    let (mut ws, mut tunn, ip, peer_ip) = client.connect_ws(args.device).await?;

    let mut dev = "tun%d".to_string();
    dev.reserve(16);
    let dev = CString::new(dev)?;
    let self_ip = CString::new(ip)?;
    let peer_ip = CString::new(peer_ip)?;
    let fd = unsafe {
        tun_alloc(dev.as_ptr(), self_ip.as_ptr(), peer_ip.as_ptr())
    };
    if fd < 0 {
        return Err(anyhow::anyhow!("Failed to allocate tun device"));
    }

    let (sender, mut receiver) = tokio::sync::mpsc::channel(100);
    std::thread::spawn(move || {
        let mut buf = [0u8; 2048];
        loop {
            let len = unsafe { libc::read(fd, buf.as_mut_ptr() as _, buf.len()) };
            if len < 0 {
                log::error!("Error reading from tun device");
                break;
            }
            if len == 0 {
                continue;
            }
            let data = &buf[..len as usize];
            if sender.blocking_send(data.to_vec()).is_err() {
                log::error!("Error sending data to channel");
                break;
            }
        }
    });

    loop {
        tokio::select! {
            Some(data) = receiver.recv() => {
                send_to_peer(&mut ws, data, &mut tunn).await;
            },
            data = ws.try_next() => {
                match data {
                    Ok(Some(Message::Binary(data))) => {
                        send_to_own(&mut ws, data, &mut tunn, fd).await;
                    },
                    Ok(Some(Message::Ping(data))) => {
                        ws.send(Message::Pong(data)).await.ok();
                    },
                    Ok(_) => (),
                    Err(e) => {
                        log::error!("Error receiving data: {:?}", e);
                        break;
                    }
                }
            },
            else => {
                log::error!("WebSocket closed");
                break;
            }
        }
    }

    let join_handle = client.get_join_handle();
    drop(client);
    let join_handle = join_handle.lock().unwrap().take().unwrap();
    join_handle.await?;

    unsafe {
        libc::close(fd);
    }

    Ok(())
}

async fn send_to_peer(ws: &mut reqwest_websocket::WebSocket, data: Vec<u8>, tunn: &mut boringtun::noise::Tunn) {
    let size = if data.len() + 32 >= 148 {
        data.len() + 32
    } else {
        148
    };
    let mut buf = vec![0u8; size];
    match tunn.encapsulate(&data[4..], &mut buf) {
        TunnResult::WriteToNetwork(buf) => {
            let msg = Message::Binary(buf.to_vec());
            let _ = ws.send(msg).await;
        },
        rest => {
            log::error!("Error encapsulating data: {:?}", rest);
        }
    }
}

async fn send_to_own(ws: &mut reqwest_websocket::WebSocket, data: Vec<u8>, tunn: &mut boringtun::noise::Tunn, fd: i32) {
    let mut buf = vec![0u8; 2048];
    match tunn.decapsulate(None, &data, &mut buf) {
        TunnResult::WriteToNetwork(buf) => {
            ws.send(Message::Binary(buf.to_vec())).await.ok();
            let len = unsafe { libc::write(fd, buf.as_ptr() as _, buf.len()) };
            if len < 0 {
                log::error!("Error writing to tun device");
            }
            Box::pin(send_to_own(ws, Vec::new(), tunn, fd)).await;
        },
        TunnResult::Done => (),
        TunnResult::WriteToTunnelV4(data, _) => {
            let mut packet = vec![0u8, 0, 8, 0];
            packet.append(&mut data.to_vec());
            let len = unsafe { libc::write(fd, packet.as_ptr() as _, packet.len()) };
            if len < 0 {
                log::error!("Error writing to tun device");
            }
        }
        rest => {
            log::error!("Error decapsulating data: {:?}", rest);
        }
    }
}
