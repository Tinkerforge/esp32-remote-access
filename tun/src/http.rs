use std::sync::Arc;

use argon2::password_hash::rand_core::{OsRng, RngCore};
use base64::Engine;
use ipnetwork::IpNetwork;
use reqwest::cookie::Jar;
use serde::{Deserialize, Serialize};
use reqwest_websocket::RequestBuilderExt;
use tabled::Tabled;

#[derive(Serialize, Deserialize)]
struct LoginSchema {
    email: String,
    login_key: Vec<u8>,
}

#[derive(Serialize, Deserialize)]
struct GetSecretResponse {
    secret: Vec<u8>,
    secret_salt: Vec<u8>,
    secret_nonce: Vec<u8>,
}

#[derive(Serialize, Deserialize)]
struct GetWgKeysResponseSchema {
    id: String,
    charger_id: String,
    charger_pub: String,
    charger_address: IpNetwork,
    web_private: Vec<u8>,
    psk: Vec<u8>,
    web_address: IpNetwork,
}

#[derive(Serialize, Deserialize)]
enum ChargerStatus {
    Disconnected = 0,
    Connected = 1,
}

impl core::fmt::Display for ChargerStatus {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            ChargerStatus::Disconnected => write!(f, "Disconnected"),
            ChargerStatus::Connected => write!(f, "Connected"),
        }
    }
}


#[derive(Serialize, Deserialize)]
struct GetChargerSchema {
    id: String,
    uid: i32,
    name: String,
    note: Option<String>,
    status: ChargerStatus,
    port: i32,
    valid: bool,
}

#[derive(Tabled)]
struct DisplayDevices {
    id: String,
    uid: i32,
    name: String,
    note: String,
    status: ChargerStatus,
    webinterface_port: i32,
    valid: bool,
}

#[derive(Clone)]
pub struct Client {
    client: reqwest::Client,
    host: String,
    secret: [u8; 32],
    logged_in: bool,
    join_handle: Arc<std::sync::Mutex<Option<tokio::task::JoinHandle<()>>>>,
}

impl Client {
    pub fn new(host: String, accept_invalid_certs: bool) -> anyhow::Result<Self> {
        if accept_invalid_certs {
            log::warn!("You are accepting invalid certificates. This is potentially dangerous!");
        }

        let client = reqwest::ClientBuilder::new()
            .cookie_store(true)
            .cookie_provider(Arc::new(Jar::default()))
            .danger_accept_invalid_certs(accept_invalid_certs)
            .build()?;

        Ok(Client {
            client,
            host,
            secret: [0u8; 32],
            join_handle: Arc::new(std::sync::Mutex::new(None)),
            logged_in: false,
        })
    }

    pub async fn get(&mut self, path: &str) -> anyhow::Result<reqwest::Response> {
        let mut resp = self
            .client
            .get(format!("https://{}{}", self.host, path))
            .send()
            .await?;
        if resp.status() == reqwest::StatusCode::UNAUTHORIZED && self.logged_in {
            self.refresh_cookies().await?;
            resp = self
                .client
                .get(format!("https://{}{}", self.host, path))
                .send()
                .await?;
        }
        Ok(resp)
    }

    pub async fn post(
        &mut self,
        path: &str,
        body: impl Into<reqwest::Body> + Clone,
    ) -> anyhow::Result<reqwest::Response> {
        log::debug!("POST: {}{}", self.host, path);
        let mut resp = self
            .client
            .post(format!("https://{}{}", self.host, path))
            .body(body.clone())
            .send()
            .await?;
        if resp.status() == reqwest::StatusCode::UNAUTHORIZED && self.logged_in {
            self.refresh_cookies().await?;
            resp = self
                .client
                .post(format!("https://{}{}", self.host, path))
                .body(body)
                .send()
                .await?;
        }

        Ok(resp)
    }

    async fn refresh_cookies(&mut self) -> anyhow::Result<()> {
        let resp = self
            .client
            .get(format!("{}{}", self.host, "/api/auth/jwt_refresh"))
            .send()
            .await?;

        if resp.status().is_success() {
            return Err(anyhow::anyhow!("Failed to refresh cookies"));
        }

        Ok(())
    }

    pub async fn login(&mut self, email: String, password: &str) -> anyhow::Result<()> {
        let resp = self
            .get("/api/auth/get_login_salt?email=frederic@tinkerforge.com")
            .await?;
        let login_salt: Vec<u8> = serde_json::from_str(&resp.text().await?)?;
        let argon = argon2::Argon2::new(
            argon2::Algorithm::Argon2id,
            argon2::Version::V0x13,
            argon2::Params::new(19 * 1024, 2, 1, Some(24)).unwrap(),
        );
        let mut login_key = vec![0u8; 24];
        if let Err(err) = argon.hash_password_into(password.as_bytes(), &login_salt, &mut login_key)
        {
            log::error!("Error hashing password: {}", err);
            return Err(anyhow::anyhow!("Error hashing password"));
        }

        let login_schema = LoginSchema { email, login_key };
        let resp = self
            .post("/api/auth/login", serde_json::to_string(&login_schema)?)
            .await?;
        if !resp.status().is_success() {
            return Err(anyhow::anyhow!(
                "Failed to login: {}",
                resp.status().as_str()
            ));
        }

        let resp = self.get("/api/user/get_secret").await?;
        if !resp.status().is_success() {
            return Err(anyhow::anyhow!(
                "Failed to get secret: {}",
                resp.status().as_str()
            ));
        }
        let secret_response: GetSecretResponse = serde_json::from_str(&resp.text().await?)?;
        let argon = argon2::Argon2::new(
            argon2::Algorithm::Argon2id,
            argon2::Version::V0x13,
            argon2::Params::new(19 * 1024, 2, 1, Some(32)).unwrap(),
        );
        let mut secret_key = vec![0u8; 32];
        if let Err(error) = argon.hash_password_into(
            password.as_bytes(),
            &secret_response.secret_salt,
            &mut secret_key,
        ) {
            log::error!("Error hashing password: {}", error);
            return Err(anyhow::anyhow!("Error hashing secret key: {}", error));
        }
        let mut secret = [0u8; libsodium_sys::crypto_box_SECRETKEYBYTES as usize];
        unsafe {
            if libsodium_sys::crypto_secretbox_open_easy(
                secret.as_mut_ptr(),
                secret_response.secret.as_ptr(),
                secret_response.secret.len() as u64,
                secret_response.secret_nonce.as_ptr(),
                secret_key.as_ptr(),
            ) == -1
            {
                return Err(anyhow::anyhow!("Failed to decrypt secret"));
            }
        }

        self.secret = secret;
        self.logged_in = true;

        Ok(())
    }

    pub async fn connect_ws(&mut self, device: uuid::Uuid) -> anyhow::Result<(reqwest_websocket::WebSocket, boringtun::noise::Tunn, String, String)> {
        let resp = self
            .client
            .get(format!("https://{}/api/charger/get_key?cid={}", self.host, device.to_string()))
            .send()
            .await?;
        if resp.status() != reqwest::StatusCode::OK {
            return Err(anyhow::anyhow!(
                "Failed to get websocket key: {}",
                resp.status().as_str()
            ));
        }
        let keys: GetWgKeysResponseSchema =
            serde_json::from_str(&resp.text().await?)?;

        let wg_private_str = self.decrypt(&keys.web_private)?;
        let psk_str = self.decrypt(&keys.psk)?;

        let engine = base64::prelude::BASE64_STANDARD;

        let mut charger_pub = [0u8; 32];
        let mut wg_private = [0u8; 32];
        let mut psk = [0u8; 32];
        engine.decode_slice(keys.charger_pub, &mut charger_pub)?;
        engine.decode_slice(&wg_private_str, &mut wg_private)?;
        engine.decode_slice(&psk_str, &mut psk)?;

        let wg_private = boringtun::x25519::StaticSecret::from(wg_private);
        let rate_limiter = boringtun::noise::rate_limiter::RateLimiter::new(&boringtun::x25519::PublicKey::from(&wg_private), 1024);
        let rate_limiter = Arc::new(rate_limiter);
        let rate_limiter_cpy = rate_limiter.clone();
        std::thread::spawn(move || {
            loop {
                rate_limiter_cpy.reset_count();
                std::thread::sleep(std::time::Duration::from_secs(1));
            }
        });

        let Ok(tunn) = boringtun::noise::Tunn::new(
            wg_private,
            boringtun::x25519::PublicKey::from(charger_pub),
            Some(psk),
            None,
            OsRng.next_u32(),
            Some(rate_limiter),
        ) else {
            return Err(anyhow::anyhow!("Failed to create tunn"));
        };

        let resp = self.client.get(format!("wss://{}/api/ws?key_id={}", self.host, keys.id))
            .upgrade()
            .send()
            .await?;
        if resp.status() != reqwest::StatusCode::SWITCHING_PROTOCOLS {
            return Err(anyhow::anyhow!(
                "Failed to upgrade websocket: {}",
                resp.status().as_str()
            ));
        }

        let ws = resp.into_websocket().await?;
        let ip = keys.web_address.to_string().replace("/32", "");
        let peer_ip = keys.charger_address.to_string().replace("/32", "");

        Ok((ws, tunn, ip, peer_ip))
    }

    pub async fn list_devices(&mut self) -> anyhow::Result<()> {
        let resp = self
            .get("/api/charger/get_chargers")
            .await?;
        if resp.status() != reqwest::StatusCode::OK {
            return Err(anyhow::anyhow!(
                "Failed to get devices: {}",
                resp.status().as_str()
            ));
        }
        let devices: Vec<GetChargerSchema> =
            serde_json::from_str(&resp.text().await?)?;

        let engine = base64::prelude::BASE64_STANDARD;
        let devices: Vec<DisplayDevices> = devices.into_iter().map(|c| {
            if !c.valid {
                return DisplayDevices {
                    id: c.id,
                    uid: c.uid,
                    name: String::new(),
                    note: String::new(),
                    status: ChargerStatus::Disconnected,
                    webinterface_port: c.port,
                    valid: c.valid,
                }
            }

            let name = engine.decode(c.name).unwrap();
            let name = self.decrypt(&name).unwrap();
            let name = String::from_utf8(name).unwrap();
            let note = engine.decode(c.note.unwrap_or_default()).unwrap();
            let note = self.decrypt(&note).unwrap();
            let note = String::from_utf8(note).unwrap();
            DisplayDevices { id: c.id, uid: c.uid, name, note, status: c.status, webinterface_port: c.port, valid: true }
        }).collect();

        let table = tabled::Table::new(devices);

        println!("{}", table);

        Ok(())
    }

    fn get_pk(&self) -> anyhow::Result<[u8; 32]> {
        let mut pk = [0u8; libsodium_sys::crypto_box_PUBLICKEYBYTES as usize];
        unsafe {
            if libsodium_sys::crypto_scalarmult_base(
                pk.as_mut_ptr(),
                self.secret.as_ptr(),
            ) == -1
            {
                return Err(anyhow::anyhow!("Failed to generate keypair"));
            }
        }
        Ok(pk)
    }

    fn decrypt(&self, data: &[u8]) -> anyhow::Result<Vec<u8>> {
        let mut buf = vec![0u8; data.len() - libsodium_sys::crypto_box_SEALBYTES as usize];
        let pk = self.get_pk()?;
        unsafe {
            if libsodium_sys::crypto_box_seal_open(
                buf.as_mut_ptr(),
                data.as_ptr(),
                data.len() as u64,
                pk.as_ptr(),
                self.secret.as_ptr(),
            ) == -1
            {
                return Err(anyhow::anyhow!("Failed to decrypt data"));
            }
        }
        Ok(buf)
    }

    pub fn get_join_handle(&self) -> Arc<std::sync::Mutex<Option<tokio::task::JoinHandle<()>>>> {
        self.join_handle.clone()
    }
}

impl Drop for Client {
    fn drop(&mut self) {
        log::debug!("Dropping client");
        let client = self.client.clone();
        let host = self.host.clone();
        let mut join_handle = self.join_handle.lock().unwrap();
        let handle = tokio::spawn(async move {
            let _ = client
                .get(format!("https://{}/api/user/logout?logout_all=false", host))
                .send()
                .await;
            log::debug!("Logged out");
        });
        join_handle.replace(handle);
    }
}
