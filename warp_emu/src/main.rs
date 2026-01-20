

use std::{sync::Arc, time::Duration};

use base64::Engine;
use log::{error, info, warn};
use simplelog::{ColorChoice, Config, LevelFilter, TermLogger, TerminalMode};
use boringtun::{noise::{Tunn, TunnResult}, x25519::{PublicKey, StaticSecret}};
use clap::Parser;
use ipnetwork::IpNetwork;
use rand::{Rng, SeedableRng, rngs::StdRng};
use base64::engine::general_purpose::STANDARD;
use serde::{Deserialize, Serialize};
use tokio::{net::UdpSocket, task, sync::Barrier};

const HOST: &str = "tf-freddy:8081";
const WG_PORT: u16 = 51820;

#[derive(Parser)]
#[command(name = "warp_emu")]
struct Args {
    target_host: String,
    /// Authentication token
    auth_token: String,
    #[arg(long, default_value = "10")]
    num: usize,
}

/// Decoded authentication token structure
/// Token format: token (32 bytes) + user_id (36 bytes) + pub_key (32 bytes) + email (variable) + hash (32 bytes)
#[derive(Debug, Clone)]
struct DecodedAuthToken {
    /// The raw 32-byte token
    pub token: String,
    /// The user ID (36 bytes, UUID format)
    pub user_id: String,
}

/// Decode an authentication token string into its components
fn decode_auth_token(token_str: &str) -> Result<DecodedAuthToken, String> {
    let bytes = bs58::decode(token_str)
        .with_alphabet(bs58::Alphabet::FLICKR)
        .into_vec()
        .map_err(|e| format!("Failed to decode base58: {}", e))?;

    // Minimum size: 32 (token) + 36 (user_id) + 32 (pub_key) + 1 (min email) + 32 (hash) = 133 bytes
    if bytes.len() < 133 {
        return Err(format!(
            "Token too short: expected at least 133 bytes, got {}",
            bytes.len()
        ));
    }

    // Extract token (first 32 bytes)
    let mut token = [0u8; 32];
    token.copy_from_slice(&bytes[0..32]);
    let token = STANDARD.encode(token);

    // Extract user_id (next 36 bytes)
    let user_id = String::from_utf8(bytes[32..68].to_vec())
        .map_err(|e| format!("Invalid user_id encoding: {}", e))?;

    Ok(DecodedAuthToken {
        token,
        user_id,
    })
}

#[derive(Serialize)]
struct Keys {
    pub web_private: Vec<u8>,
    pub psk: Vec<u8>,
    pub charger_public: String,
    pub web_address: IpNetwork,
    pub charger_address: IpNetwork,
    pub connection_no: u16,
}

#[derive(Serialize)]
struct ChargerSchema {
    pub uid: String,
    pub charger_pub: String,
    pub wg_charger_ip: IpNetwork,
    pub wg_server_ip: IpNetwork,
    pub psk: String,
}

#[derive(Serialize)]
struct AddChargerWithTokenSchema {
    pub token: String,
    pub user_id: String,
    pub charger: ChargerSchema,
    pub keys: [Keys; 5],
    pub name: String,
    pub note: String,
}

fn generate_random_keys() -> Keys {
    let mut rng = rand::rng();

    Keys {
        web_private: (0..32).map(|_| rng.random()).collect(),
        psk: (0..32).map(|_| rng.random()).collect(),
        charger_public: STANDARD.encode((0..32).map(|_| rng.random::<u8>()).collect::<Vec<u8>>()),
        web_address: "10.0.0.1/24".parse().unwrap(),
        charger_address: "10.0.0.2/24".parse().unwrap(),
        connection_no: rng.random(),
    }
}

fn generate_random_add_charger_schema(user_id: &str, token: &str, pub_key: String, psk: &[u8; 32]) -> AddChargerWithTokenSchema {
    let mut rng = rand::rng();

    let uid: String = bs58::encode(rng.random::<[u8; 4]>()).with_alphabet(bs58::Alphabet::FLICKR).into_string();

    let charger = ChargerSchema {
        uid: uid.clone(),
        charger_pub: pub_key,
        wg_charger_ip: "10.1.0.1/24".parse().unwrap(),
        wg_server_ip: "10.1.0.2/24".parse().unwrap(),
        psk: STANDARD.encode(psk),
    };

    AddChargerWithTokenSchema {
        token: token.to_string(),
        user_id: user_id.to_string(),
        charger,
        keys: [
            generate_random_keys(),
            generate_random_keys(),
            generate_random_keys(),
            generate_random_keys(),
            generate_random_keys(),
        ],
        name: format!("Charger-{}", uid),
        note: String::from("Random test charger"),
    }
}

#[derive(Deserialize)]
pub struct AddChargerResponseSchema {
    pub management_pub: String,
    pub charger_uuid: String,
    pub charger_password: String,
    pub user_id: String,
}

#[derive(Serialize)]
pub struct ConfiguredUser {
    pub email: Option<String>,
    pub user_id: Option<String>,
    pub name: Option<String>,
}

#[derive(Serialize)]
pub struct ManagementDataVersion2 {
    pub id: String,
    pub password: String,
    pub port: u16,
    pub firmware_version: String,
    pub configured_users: Vec<ConfiguredUser>,
    pub mtu: Option<u16>,
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
pub enum ManagementDataVersion {
    V2(ManagementDataVersion2),
}

#[derive(Serialize)]
pub struct ManagementSchema {
    pub id: Option<i32>,
    pub password: Option<String>,
    pub data: ManagementDataVersion,
}

#[derive(Deserialize, Debug)]
pub struct ManagementResponseSchema {
    pub time: u64,
    pub configured_users: Vec<i32>,
    pub configured_users_emails: Vec<String>,
    pub configured_users_uuids: Vec<String>,
    pub uuid: Option<String>,
}

struct EmuCharger {
    uuid: String,
    password: String,
    user_id: String,
    barrier: Arc<Barrier>,
    socket: UdpSocket,
    tun: Tunn,
    rate_limiter: Arc<boringtun::noise::rate_limiter::RateLimiter>,
}

impl EmuCharger {
    async fn new(user_id: &str, token: &str, barrier: Arc<Barrier>) -> anyhow::Result<Self> {
        let mut rng = StdRng::from_os_rng();

        let priv_key: [u8; 32] = rng.random();
        let priv_key = StaticSecret::from(priv_key);
        let pub_key = PublicKey::from(&priv_key);

        let psk: [u8; 32] = rng.random();

        let schema = generate_random_add_charger_schema(user_id, token, STANDARD.encode(pub_key.as_bytes()), &psk);

        let client = reqwest::Client::new();
        let response = client
            .put(format!("https://{}/api/add_with_token", HOST))
            .json(&schema)
            .send()
            .await?;

        let body = response.json::<AddChargerResponseSchema>().await?;

        // Decode the management public key from the server
        let management_pub_bytes: [u8; 32] = STANDARD
            .decode(&body.management_pub)?
            .try_into()
            .map_err(|_| anyhow::anyhow!("Invalid management public key length"))?;
        let management_pub = PublicKey::from(management_pub_bytes);

        let socket = UdpSocket::bind("0.0.0.0:0").await?;
        let rate_limiter = boringtun::noise::rate_limiter::RateLimiter::new(&pub_key, 1200);
        let rate_limiter = Arc::new(rate_limiter);

        let tun = Tunn::new(
            priv_key,
            management_pub,
            Some(psk),
            Some(10), // Persistent keepalive every 10 seconds
            rng.random(),
            Some(rate_limiter.clone()),
        );

        Ok(Self {
            uuid: body.charger_uuid,
            password: body.charger_password,
            user_id: user_id.to_string(),
            barrier,
            socket,
            tun,
            rate_limiter,
        })
    }

    /// Start a WireGuard management connection to the server.
    /// This establishes a WireGuard tunnel over UDP to port 51820.
    async fn start_management_connection(&mut self, server_addr: &str) -> anyhow::Result<()> {
        // Resolve the server address
        let socket_addr = format!("{}:{}", server_addr, WG_PORT);
        self.socket.connect(socket_addr).await?;

        self.rate_limiter.reset_count();

        let mut buf = vec![0u8; 2048];
        // Send handshake initiation
        match self.tun.format_handshake_initiation(&mut buf, true) {
            TunnResult::WriteToNetwork(data) => {
                self.socket.send(data).await?;
            },
            TunnResult::Done => {
                // Nothing to send
            }
            other => {
                return Err(anyhow::anyhow!(
                    "Unexpected result from format_handshake_initiation: {:?}",
                    other
                ));
            }
        }

        // Wait for handshake response with timeout
        let mut recv_buf = [0u8; 2048];
        let handshake_timeout = Duration::from_secs(5);

        loop {
            match tokio::time::timeout(handshake_timeout, self.socket.recv(&mut recv_buf)).await {
                Ok(Ok(n)) => {
                    match self.tun.decapsulate(None, &recv_buf[..n], &mut buf) {
                        TunnResult::WriteToNetwork(data) => {
                            // Send response (typically the handshake response)
                            self.socket.send(data).await?;
                            break;
                        }
                        TunnResult::Done => {
                            // Handshake complete or keepalive received
                            if self.tun.time_since_last_handshake().is_some() {
                                break;
                            }
                        }
                        TunnResult::Err(e) => {
                            return Err(anyhow::anyhow!("WireGuard error: {:?}", e));
                        }
                        TunnResult::WriteToTunnelV4(_, _) | TunnResult::WriteToTunnelV6(_, _) => {
                            // Received encapsulated data - handshake is complete
                            break;
                        }
                    }
                }
                Ok(Err(e)) => {
                    return Err(anyhow::anyhow!("Socket receive error: {}", e));
                }
                Err(_) => {
                    return Err(anyhow::anyhow!("Handshake timeout"));
                }
            }
        }
        Ok(())
    }

    async fn management_request(&self) -> anyhow::Result<ManagementResponseSchema> {
        let data = ManagementDataVersion2 {
            id: self.uuid.clone(),
            password: self.password.clone(),
            port: 8080,
            firmware_version: "2.0.0".to_string(),
            configured_users: vec![ConfiguredUser {
                email: None,
                user_id: Some(self.user_id.clone()),
                name: Some("Emulated Charger".to_string()),
            }],
            mtu: Some(1420),
        };

        let schema = ManagementSchema {
            id: None,
            password: None,
            data: ManagementDataVersion::V2(data),
        };

        let client = reqwest::Client::new();
        let response = client
            .put(format!("https://{}/api/management", HOST))
            .json(&schema)
            .send()
            .await?;

        let body = response.json::<ManagementResponseSchema>().await?;
        Ok(body)
    }
}

impl Drop for EmuCharger {

    fn drop(&mut self) {

        #[derive(Serialize)]
        struct SelfdestructSchema {
            pub id: Option<i32>,
            pub uuid: Option<String>,
            pub password: String,
        }

        let schema = SelfdestructSchema {
            id: None,
            uuid: Some(self.uuid.clone()),
            password: self.password.clone(),
        };
        let client = reqwest::Client::new();
        let barrier = self.barrier.clone();
        task::spawn(async move {
            let mut resp = client
                .delete(format!("https://{}/api/selfdestruct", HOST))
                .json(&schema)
                .send()
                .await;

            while resp.is_err() || resp.as_ref().unwrap().status() == reqwest::StatusCode::INTERNAL_SERVER_ERROR {
                if let Ok(r) = &resp {
                    warn!("Selfdestruct failed with status: {}", r.status());
                }

                let mut rng = StdRng::from_os_rng();
                tokio::time::sleep(Duration::from_millis(rng.random_range(1000..5000))).await;
                resp = client
                .delete(format!("https://{}/api/selfdestruct", HOST))
                .json(&schema)
                .send()
                .await;
            }

            barrier.wait().await;
        });

    }
}

#[tokio::main]
async fn main() {
    TermLogger::init(
        LevelFilter::Info,
        Config::default(),
        TerminalMode::Mixed,
        ColorChoice::Auto,
    )
    .expect("Failed to initialize logger");

    let args = Args::parse();
    let token = decode_auth_token(&args.auth_token).unwrap();
    let target_host = args.target_host.split(':').next().unwrap_or(&args.target_host).to_string();


    // Create all chargers and collect them
    let mut poll_tasks= Vec::with_capacity(args.num);
    let mut creation_tasks = Vec::with_capacity(args.num);

    let barrier = Arc::new(Barrier::new(args.num + 1));
    for _ in 0..args.num {
        let token = token.clone();
        let barrier = barrier.clone();

        creation_tasks.push(task::spawn(async move {
            let charger = EmuCharger::new(&token.user_id, &token.token, barrier).await;
            let charger = match charger {
                Ok(c) => c,
                Err(e) => {
                    error!("Failed to create charger: {}", e);
                    return None;
                }
            };

            Some(charger)
        }));
    }

    let (broadcast, _) = tokio::sync::broadcast::channel(1);
    // Collect all successfully created chargers
    for task in creation_tasks {
        let mut broadcast_rx = broadcast.subscribe();
        let target_host = target_host.clone();
        if let Ok(Some(mut charger)) = task.await {
            poll_tasks.push(task::spawn(async move {
                // Make a management request
                if let Err(e) = charger.management_request().await {
                    error!("Management request failed: {}", e);
                }

                // Start WireGuard management connection
                if let Err(e) = charger.start_management_connection(&target_host).await {
                    error!("1 Management connection failed: {}, self addr: {}", e, charger.socket.local_addr().unwrap());
                    return;
                }

                // Polling loop
                let mut rng = StdRng::from_os_rng();
                loop {
                    let sleep_secs = rng.random_range(5..=30);
                    tokio::select! {
                        _ = tokio::time::sleep(Duration::from_secs(sleep_secs)) => {
                            log::info!("Charger {} sending keepalive...", charger.uuid);

                            // Make a management request
                            if let Err(e) = charger.management_request().await {
                                error!("Management request failed: {}", e);
                            }
                            // Start WireGuard management connection
                            if let Err(e) = charger.start_management_connection(&target_host).await {
                                error!("2 Management connection failed: {}, self addr: {}", e, charger.socket.local_addr().unwrap());
                                return;
                            }
                            // Send keepalive
                            // match charger.tun.format_handshake_initiation(&mut buf, false) {
                            //     TunnResult::WriteToNetwork(data) => {
                            //         if let Err(e) = charger.socket.send(data).await {
                            //             eprintln!("Failed to send keepalive: {}", e);
                            //         }
                            //     },
                            //     TunnResult::Done => {
                            //         // Nothing to send
                            //     }
                            //     other => {
                            //         eprintln!("Unexpected result from format_handshake_initiation: {:?}", other);
                            //     }
                            // }
                        }
                        _ = broadcast_rx.recv() => {
                            // Received shutdown signal
                            break;
                        }
                    }
                }
            }));
        }
    }

    info!("Created {} chargers, starting keepalive polling...", poll_tasks.len());

    info!("Waiting for SIGINT (Ctrl+C) to cleanup...");

    // Wait for SIGINT
    tokio::signal::ctrl_c().await.expect("Failed to listen for Ctrl+C");

    // Spawn a task to force quit on second Ctrl+C
    tokio::spawn(async {
        tokio::signal::ctrl_c().await.expect("Failed to listen for Ctrl+C");
        warn!("Force quitting...");
        std::process::exit(1);
    });

    let _ = broadcast.send(());

    // Cleanup: drop all chargers and wait for selfdestruct to complete
    barrier.wait().await;

    info!("Cleanup complete.");
}
