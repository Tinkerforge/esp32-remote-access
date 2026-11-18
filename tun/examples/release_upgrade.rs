//! Synthetic IPv4 device and client for scripts/smoke-release-upgrade.sh.
use std::{
    net::{Ipv4Addr, SocketAddrV4},
    path::PathBuf,
    time::Duration,
};

use anyhow::{Context, Result, bail, ensure};
use base64::{Engine, prelude::BASE64_STANDARD as BASE64};
use boringtun::{
    noise::{Tunn, TunnResult},
    x25519::{PublicKey, StaticSecret},
};
use clap::{Parser, Subcommand};
use futures_util::{SinkExt, TryStreamExt};
use ipnetwork::Ipv4Network;
use reqwest::{Certificate, Client, Response};
use reqwest_websocket::{Message, Upgrade, WebSocket};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::json;
use tokio::{net::UdpSocket, time::timeout};
use uuid::Uuid;

const EMAIL: &str = "upgrade-smoke@example.test";
const LOGIN_KEY: [u8; 24] = [17; 24];
const ACCOUNT_SECRET: [u8; 32] = [18; 32];
const MANAGEMENT_PRIVATE: [u8; 32] = [19; 32];
const MANAGEMENT_PSK: [u8; 32] = [20; 32];
const WEB_PRIVATE: [u8; 32] = [21; 32];
const WEB_PSK: [u8; 32] = [22; 32];
const DEVICE_PRIVATE: [u8; 32] = [23; 32];
const WEB_ADDRESS: &str = "10.123.0.3/24";
const DEVICE_ADDRESS: &str = "10.123.0.2/32";
const DEVICE_PORT: u16 = 8080;
const PROBE_TIMEOUT: Duration = Duration::from_secs(15);

#[derive(Parser)]
struct Args {
    url: String,
    certificate: PathBuf,
    #[command(subcommand)]
    phase: Phase,
}

#[derive(Subcommand)]
enum Phase {
    Account,
    Register {
        registration: PathBuf,
    },
    Probe {
        registration: PathBuf,
        udp_port: u16,
    },
}

#[derive(Deserialize, Serialize)]
struct Registration {
    management_pub: String,
    charger_uuid: Uuid,
    charger_password: String,
    user_id: Uuid,
}

#[derive(Deserialize)]
struct ConnectionKey {
    id: Uuid,
    charger_id: Uuid,
    charger_pub: String,
    web_private: Vec<u8>,
    psk: Vec<u8>,
    web_address: Ipv4Network,
    charger_address: Ipv4Network,
}

fn public_key(secret: [u8; 32]) -> PublicKey {
    PublicKey::from(&StaticSecret::from(secret))
}

fn seal(data: &[u8]) -> Result<Vec<u8>> {
    let recipient = public_key(ACCOUNT_SECRET);
    let mut sealed = vec![0; data.len() + libsodium_sys::crypto_box_SEALBYTES as usize];
    // The output buffer includes the seal overhead and the recipient key is 32 bytes.
    let status = unsafe {
        libsodium_sys::crypto_box_seal(
            sealed.as_mut_ptr(),
            data.as_ptr(),
            data.len() as u64,
            recipient.as_bytes().as_ptr(),
        )
    };
    ensure!(status == 0, "could not seal test data");
    Ok(sealed)
}

fn open_key(sealed: &[u8]) -> Result<[u8; 32]> {
    let recipient = public_key(ACCOUNT_SECRET);
    // Devices seal the padded base64 representation of their 32-byte keys.
    let mut encoded = [0; 44];
    ensure!(
        sealed.len() == encoded.len() + libsodium_sys::crypto_box_SEALBYTES as usize,
        "wrong saved key length"
    );
    // The ciphertext length check ensures the plaintext fits in encoded.
    let status = unsafe {
        libsodium_sys::crypto_box_seal_open(
            encoded.as_mut_ptr(),
            sealed.as_ptr(),
            sealed.len() as u64,
            recipient.as_bytes().as_ptr(),
            ACCOUNT_SECRET.as_ptr(),
        )
    };
    ensure!(status == 0, "could not open saved key");
    decode_key(encoded)
}

fn decode_key(encoded: impl AsRef<[u8]>) -> Result<[u8; 32]> {
    BASE64
        .decode(encoded)?
        .try_into()
        .map_err(|_| anyhow::anyhow!("wrong decoded key length"))
}

async fn checked(response: Response) -> Result<Response> {
    if response.status().is_success() {
        return Ok(response);
    }
    let status = response.status();
    let url = response.url().clone();
    bail!("{url} returned {status}: {}", response.text().await?);
}

async fn json_reply<T: DeserializeOwned>(response: Response) -> Result<T> {
    Ok(checked(response).await?.json().await?)
}

async fn create_account(client: &Client, url: &str) -> Result<()> {
    checked(
        client
            .post(format!("{url}/api/auth/register"))
            .json(&json!({
                "email": EMAIL, "name": "Upgrade Smoke", "login_key": LOGIN_KEY,
                "login_salt": vec![1; 24], "secret": vec![2; 32], "secret_nonce": vec![3; 24],
                "secret_salt": vec![4; 24],
            }))
            .send()
            .await?,
    )
    .await?;
    Ok(())
}

async fn login(client: &Client, url: &str) -> Result<()> {
    checked(
        client
            .post(format!("{url}/api/auth/login"))
            .json(&json!({ "email": EMAIL, "login_key": LOGIN_KEY }))
            .send()
            .await?,
    )
    .await?;
    Ok(())
}

async fn register(client: &Client, url: &str) -> Result<Registration> {
    let keys: Vec<_> = (0..5)
        .map(|slot| {
            Ok(json!({
                "connection_no": slot,
                "charger_public": BASE64.encode(public_key(DEVICE_PRIVATE).as_bytes()),
                "web_private": seal(BASE64.encode(WEB_PRIVATE).as_bytes())?,
                "psk": seal(BASE64.encode(WEB_PSK).as_bytes())?,
                "web_address": WEB_ADDRESS,
                "charger_address": DEVICE_ADDRESS,
            }))
        })
        .collect::<Result<_>>()?;
    json_reply(
        client
            .put(format!("{url}/api/charger/add"))
            .json(&json!({
                "charger": {
                    "uid": "112J",
                    "charger_pub": BASE64.encode(public_key(MANAGEMENT_PRIVATE).as_bytes()),
                    "wg_charger_ip": "10.1.0.1/24",
                    "wg_server_ip": "10.1.0.2/24",
                    "psk": BASE64.encode(MANAGEMENT_PSK),
                },
                "keys": keys,
                "name": BASE64.encode(seal(b"Upgrade smoke device")?),
                "note": "",
            }))
            .send()
            .await?,
    )
    .await
}

fn network_packet(result: TunnResult<'_>) -> Result<&[u8]> {
    match result {
        TunnResult::WriteToNetwork(packet) => Ok(packet),
        other => bail!("expected a WireGuard network packet, got {other:?}"),
    }
}

async fn open_websocket(client: &Client, endpoint: &str) -> Result<WebSocket> {
    let response = client.get(endpoint).upgrade().send().await?;
    ensure!(
        response.status() == reqwest::StatusCode::SWITCHING_PROTOCOLS,
        "{endpoint} returned {} instead of upgrading",
        response.status()
    );
    Ok(response.into_websocket().await?)
}

async fn next_binary(ws: &mut WebSocket) -> Result<Vec<u8>> {
    while let Some(message) = ws.try_next().await? {
        if let Message::Binary(packet) = message {
            return Ok(packet.to_vec());
        }
    }
    bail!("relay WebSocket closed")
}

async fn udp_peer(port: u16) -> Result<UdpSocket> {
    let socket = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).await?;
    socket.connect((Ipv4Addr::LOCALHOST, port)).await?;
    Ok(socket)
}

async fn open_management(
    client: &Client,
    url: &str,
    registration: &Registration,
    udp_port: u16,
) -> Result<(UdpSocket, Tunn)> {
    checked(
        client
            .put(format!("{url}/api/management"))
            .header("User-Agent", "Tinkerforge-WARP2_Charger/2.13.6")
            .json(&json!({ "data": { "V2": {
                "id": registration.charger_uuid,
                "password": registration.charger_password,
                "firmware_version": "2.13.6",
                "port": DEVICE_PORT,
                "configured_users": [{ "user_id": registration.user_id }],
                "mtu": 1420,
            } } }))
            .send()
            .await?,
    )
    .await?;

    let socket = udp_peer(udp_port).await?;
    let mut tunnel = Tunn::new(
        StaticSecret::from(MANAGEMENT_PRIVATE),
        PublicKey::from(decode_key(&registration.management_pub)?),
        Some(MANAGEMENT_PSK),
        None,
        1,
        None,
    );
    let (mut input, mut output) = ([0; 2048], [0; 2048]);
    socket
        .send(network_packet(
            tunnel.format_handshake_initiation(&mut output, true),
        )?)
        .await?;
    let size = socket.recv(&mut input).await?;
    socket
        .send(network_packet(tunnel.decapsulate(
            None,
            &input[..size],
            &mut output,
        ))?)
        .await?;
    Ok((socket, tunnel))
}

async fn answer_connect(
    management: UdpSocket,
    mut tunnel: Tunn,
    device: &UdpSocket,
    charger_id: Uuid,
) -> Result<()> {
    let (mut input, mut output) = ([0; 2048], [0; 2048]);
    loop {
        let size = management.recv(&mut input).await?;
        match tunnel.decapsulate(None, &input[..size], &mut output) {
            TunnResult::Done => continue,
            TunnResult::WriteToNetwork(packet) => {
                management.send(packet).await?;
            }
            TunnResult::WriteToTunnelV4(packet, _) => {
                // The IPv4/UDP management command contains an 8-byte header,
                // a 4-byte Connect command (0), a slot number and a nonce.
                let body = packet.get(28..).context("short IPv4 management packet")?;
                ensure!(body.len() == 32, "wrong management packet length");
                ensure!(
                    body[..4] == [0x34, 0x12, 24, 0] && body[6..12] == [1, 0, 0, 0, 0, 0],
                    "expected a version 1 Connect command"
                );
                // The UDP port discovery reply echoes the slot and nonce
                // from Connect and is sent from the separate user tunnel socket.
                let mut response = vec![0x34, 0x12, 36, 0, 0, 0, 1, 0];
                response.extend_from_slice(&charger_id.as_u128().to_le_bytes());
                response.extend_from_slice(&body[12..]);
                device.send(&response).await?;
                return Ok(());
            }
            other => bail!("unexpected management tunnel packet: {other:?}"),
        }
    }
}

// A minimal IPv4 UDP datagram. IPv4 permits a zero UDP checksum.
fn datagram(src: SocketAddrV4, dst: SocketAddrV4, payload: &[u8]) -> Vec<u8> {
    let mut packet = vec![0; 28];
    packet[0] = 0x45;
    packet[2..4].copy_from_slice(&(28_u16 + payload.len() as u16).to_be_bytes());
    packet[8] = 64;
    packet[9] = 17;
    packet[12..16].copy_from_slice(&src.ip().octets());
    packet[16..20].copy_from_slice(&dst.ip().octets());
    let sum: u32 = packet[..20]
        .as_chunks::<2>()
        .0
        .iter()
        .map(|&word| u32::from(u16::from_be_bytes(word)))
        .sum();
    let folded = (sum & 0xffff) + (sum >> 16);
    let checksum = !((folded + (folded >> 16)) as u16);
    packet[10..12].copy_from_slice(&checksum.to_be_bytes());
    packet[20..22].copy_from_slice(&src.port().to_be_bytes());
    packet[22..24].copy_from_slice(&dst.port().to_be_bytes());
    packet[24..26].copy_from_slice(&(8_u16 + payload.len() as u16).to_be_bytes());
    packet.extend_from_slice(payload);
    packet
}

async fn exchange(mut ws: WebSocket, socket: UdpSocket, key: ConnectionKey) -> Result<()> {
    ensure!(
        key.web_address == WEB_ADDRESS.parse()?,
        "web address changed"
    );
    ensure!(
        key.charger_address == DEVICE_ADDRESS.parse()?,
        "device address changed"
    );
    let mut web = Tunn::new(
        StaticSecret::from(open_key(&key.web_private)?),
        PublicKey::from(decode_key(&key.charger_pub)?),
        Some(open_key(&key.psk)?),
        None,
        2,
        None,
    );
    let mut device = Tunn::new(
        StaticSecret::from(DEVICE_PRIVATE),
        public_key(WEB_PRIVATE),
        Some(WEB_PSK),
        None,
        3,
        None,
    );

    let (mut input, mut output) = ([0; 2048], [0; 2048]);
    let initiation = network_packet(web.format_handshake_initiation(&mut output, true))?.to_vec();
    let size = loop {
        // Port discovery and WebSocket session setup are asynchronous.
        ws.send(Message::Binary(initiation.clone().into())).await?;
        if let Ok(size) = timeout(Duration::from_secs(1), socket.recv(&mut input)).await {
            break size?;
        }
    };
    socket
        .send(network_packet(device.decapsulate(
            None,
            &input[..size],
            &mut output,
        ))?)
        .await?;
    let response = next_binary(&mut ws).await?;
    let keepalive = network_packet(web.decapsulate(None, &response, &mut output))?;
    ws.send(Message::Binary(keepalive.to_vec().into())).await?;

    let web_endpoint = SocketAddrV4::new(key.web_address.ip(), 12345);
    let device_endpoint = SocketAddrV4::new(key.charger_address.ip(), DEVICE_PORT);
    let request = datagram(web_endpoint, device_endpoint, b"upgrade smoke request");
    let encrypted = network_packet(web.encapsulate(&request, &mut output))?;
    ws.send(Message::Binary(encrypted.to_vec().into())).await?;
    loop {
        let size = socket.recv(&mut input).await?;
        if input[..size] == initiation {
            continue;
        }
        match device.decapsulate(None, &input[..size], &mut output) {
            TunnResult::Done => continue, // The handshake's empty keepalive.
            TunnResult::WriteToTunnelV4(packet, _) if packet == request => break,
            other => bail!("device did not receive the expected request: {other:?}"),
        }
    }

    let reply = datagram(device_endpoint, web_endpoint, b"upgrade smoke reply");
    socket
        .send(network_packet(device.encapsulate(&reply, &mut output))?)
        .await?;
    let encrypted = next_binary(&mut ws).await?;
    match web.decapsulate(None, &encrypted, &mut output) {
        TunnResult::WriteToTunnelV4(packet, _) if packet == reply => Ok(()),
        other => bail!("client did not receive the expected device reply: {other:?}"),
    }
}

async fn probe(client: &Client, url: &str, registration: Registration, port: u16) -> Result<()> {
    let id = registration.charger_uuid;
    let key: ConnectionKey = json_reply(
        client
            .get(format!("{url}/api/charger/get_key?cid={id}"))
            .send()
            .await?,
    )
    .await
    .context("retrieving saved tunnel key")?;
    ensure!(
        key.charger_id == id,
        "relay returned a key for another device"
    );

    let (management, tunnel) = timeout(
        PROBE_TIMEOUT,
        open_management(client, url, &registration, port),
    )
    .await
    .context("management tunnel timed out")?
    .context("establishing management tunnel")?;
    let device = udp_peer(port).await?;
    let ws = open_websocket(client, &format!("{url}/api/ws?key_id={}", key.id)).await?;
    timeout(
        PROBE_TIMEOUT,
        answer_connect(management, tunnel, &device, id),
    )
    .await
    .context("timed out waiting for the device's Connect command")??;
    timeout(PROBE_TIMEOUT, exchange(ws, device, key))
        .await
        .context("timed out exchanging traffic through the user tunnel")??;
    println!("Exchanged tunnel traffic with {id}");
    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    let client = Client::builder()
        .http1_only()
        .cookie_store(true)
        .no_proxy()
        .tls_certs_merge([Certificate::from_pem(&std::fs::read(args.certificate)?)?])
        .timeout(Duration::from_secs(10))
        .build()?;
    let url = &args.url;

    ensure!(
        unsafe { libsodium_sys::sodium_init() } >= 0,
        "initializing libsodium"
    );
    if !matches!(args.phase, Phase::Account) {
        login(&client, url).await.context("logging in")?;
    }
    match args.phase {
        Phase::Account => create_account(&client, url)
            .await
            .context("creating test account"),
        Phase::Register { registration } => {
            let device = register(&client, url)
                .await
                .context("registering test device")?;
            println!("Registered {}", device.charger_uuid);
            std::fs::write(registration, serde_json::to_vec(&device)?)?;
            Ok(())
        }
        Phase::Probe {
            registration,
            udp_port,
        } => {
            let device = serde_json::from_slice(&std::fs::read(registration)?)?;
            probe(&client, url, device, udp_port).await
        }
    }
}
