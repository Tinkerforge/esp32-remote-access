use std::sync::Arc;

use argon2::password_hash::rand_core::{OsRng, RngCore};
use base64::Engine;
use ipnetwork::IpNetwork;
use serde::{Deserialize, Serialize};
use reqwest_websocket::RequestBuilderExt;

#[derive(Serialize, Deserialize, Default)]
pub struct Cache {
    pub cookie: String,
    pub host: String,
    pub secret: String,
}

#[derive(Serialize, Deserialize)]
struct LoginSchema {
    email: String,
    login_key: Vec<u8>,
}

#[derive(Serialize, Deserialize)]
pub struct GetSecretResponse {
    secret: Vec<u8>,
    secret_salt: Vec<u8>,
    secret_nonce: Vec<u8>,
}

#[derive(Serialize, Deserialize)]
pub struct GetWgKeysResponseSchema {
    id: String,
    charger_id: String,
    charger_pub: String,
    charger_address: IpNetwork,
    web_private: Vec<u8>,
    psk: Vec<u8>,
    web_address: IpNetwork,
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
    pub fn new(cache: Cache, accept_invalid_certs: bool) -> anyhow::Result<Self> {
        if accept_invalid_certs {
            log::warn!("You are accepting invalid certificates. This is potentially dangerous!");
        }

        let host = cache.host;
        let jar = reqwest::cookie::Jar::default();
        jar.add_cookie_str(&cache.cookie, &format!("https://{}", host).parse()?);

        let client = reqwest::ClientBuilder::new()
            .cookie_store(true)
            .cookie_provider(Arc::new(jar))
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

        let mut wg_private_str = vec![0u8; keys.web_private.len() - libsodium_sys::crypto_box_SEALBYTES as usize];
        let mut psk_str = vec![0u8; keys.psk.len() - libsodium_sys::crypto_box_SEALBYTES as usize];
        unsafe {
            if libsodium_sys::crypto_box_seal_open(wg_private_str.as_mut_ptr(), keys.web_private.as_ptr(), keys.web_private.len() as u64, pk.as_ptr(), self.secret.as_ptr()) == -1 {
                return Err(anyhow::anyhow!("Failed to decrypt private key"));
            }
            if libsodium_sys::crypto_box_seal_open(psk_str.as_mut_ptr(), keys.psk.as_ptr(), keys.psk.len() as u64, pk.as_ptr(), self.secret.as_ptr()) == -1 {
                return Err(anyhow::anyhow!("Failed to decrypt psk"));
            }
        }
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
