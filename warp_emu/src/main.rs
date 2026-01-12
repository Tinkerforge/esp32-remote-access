

use std::{sync::{Arc, atomic}, time::Duration};

use base64::Engine;
use boringtun::{noise::Tunn, x25519::{PublicKey, StaticSecret}};
use clap::Parser;
use ipnetwork::IpNetwork;
use rand::{Rng, SeedableRng, rngs::StdRng};
use base64::engine::general_purpose::STANDARD;
use serde::{Deserialize, Serialize};
use tokio::{task, sync::Barrier};

const HOST: &str = "mystaging.warp-charger.com";

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

fn generate_random_add_charger_schema(user_id: &str, token: &str, pub_key: String) -> AddChargerWithTokenSchema {
    let mut rng = rand::rng();

    let uid: String = bs58::encode(rng.random::<[u8; 4]>()).with_alphabet(bs58::Alphabet::FLICKR).into_string();

    let charger = ChargerSchema {
        uid: uid.clone(),
        charger_pub: pub_key,
        wg_charger_ip: "10.1.0.1/24".parse().unwrap(),
        wg_server_ip: "10.1.0.2/24".parse().unwrap(),
        psk: STANDARD.encode((0..32).map(|_| rng.random::<u8>()).collect::<Vec<u8>>()),
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

struct EmuCharger {
    uuid: String,
    password: String,
    tun: Tunn,
    barrier: Arc<Barrier>,
}

impl EmuCharger {
    async fn new(user_id: &str, token: &str, barrier: Arc<Barrier>, ) -> anyhow::Result<Self> {
        let mut rng = StdRng::from_os_rng();

        let priv_key: [u8; 32] = rng.random();
        let priv_key = StaticSecret::from(priv_key);
        let pub_key = PublicKey::from(&priv_key);

        let psk: [u8; 32] = rng.random();

        let schema = generate_random_add_charger_schema(user_id, token, STANDARD.encode(pub_key.as_bytes()));

        let client = reqwest::Client::new();
        let response = client
            .put(format!("https://{}/api/add_with_token", HOST))
            .json(&schema)
            .send()
            .await?;

        let body = response.json::<AddChargerResponseSchema>().await?;

        Ok(Self {
            uuid: body.charger_uuid,
            password: body.charger_password,
            tun: Tunn::new(
                priv_key,
                pub_key,
                Some(psk),
                None,
                rng.random(),
                None,
            ).unwrap(),
            barrier,
        })
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
                    println!("Selfdestruct failed with status: {}", r.status());
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
    let args = Args::parse();
    let token = decode_auth_token(&args.auth_token).unwrap();
    let barrier = Arc::new(Barrier::new(args.num + 1));
    let num_created= Arc::new(atomic::AtomicUsize::new(0));
    for _ in 0..args.num {
        let barrier = barrier.clone();
        let token = token.clone();
        let num_created = num_created.clone();
        task::spawn(async move {
            let charger = EmuCharger::new(&token.user_id, &token.token, barrier.clone()).await;
            if charger.is_err() {
                barrier.wait().await;
                return;
            }
            num_created.fetch_add(1, atomic::Ordering::SeqCst);
        });

    }
    barrier.wait().await;
    println!("Created {} chargers", num_created.load(atomic::Ordering::SeqCst));
}
