use std::net::Ipv4Addr;

use anyhow::Error;
use argon2::password_hash::rand_core::OsRng;
use backend::{routes::{charger::add::{AddChargerResponseSchema, AddChargerSchema, ChargerSchema}, management::{ManagementDataVersion, ManagementDataVersion1, ManagementSchema}, user::get_secret::GetSecretResponse}, x25519::{PublicKey, StaticSecret}};
use base64::Engine;
use ipnetwork::{IpNetwork, Ipv4Network};

use crate::{crypto::generate_hash, State, ID};

fn create_http_client() -> anyhow::Result<reqwest::Client> {
    let client = reqwest::Client::builder()
        .danger_accept_invalid_certs(true)
        .build()?;

    Ok(client)
}

pub async fn get_login_salt(email: &String, base_url: &String) -> anyhow::Result<Vec<u8>> {
    let url = format!("{}/api/auth/get_login_salt?email={}", base_url, email);
    let salt = reqwest::get(url)
        .await?
        .json()
        .await?;

    Ok(salt)
}

pub async fn login(email: String, login_key: Vec<u8>, base_url: &String) -> anyhow::Result<String> {
    use backend::routes::auth::login::LoginSchema;

    let schema = LoginSchema {
        email,
        login_key
    };
    let client = create_http_client()?;
    let resp = client.post(format!("{}/api/auth/login", base_url))
        .body(serde_json::to_string(&schema)?)
        .send()
        .await?;
    let mut cookie = String::new();
    for c in resp.cookies() {
        if c.name() == "access_token" {
            cookie = c.value().to_owned();
            break;
        }
    }

    Ok(cookie)
}

pub async fn get_secret(access_token: &String, password: &String, base_url: &String) -> anyhow::Result<Vec<u8>> {
    let client = create_http_client()?;
    let resp = client.request(reqwest::Method::GET, format!("{}/api/user/get_secret", base_url))
        .header(reqwest::header::COOKIE, format!("access_token={}", access_token))
        .send()
        .await?;

    if resp.status() != 200 {
        return Err(Error::msg("getting secret failed"));
    }
    let secret_resp: GetSecretResponse = resp.json().await?;

    let secret_key = generate_hash(password.as_bytes(), &secret_resp.secret_salt, Some(libsodium_sys::crypto_secretbox_KEYBYTES as usize))?;

    let mut secret = vec![0u8; libsodium_sys::crypto_box_SECRETKEYBYTES as usize];
    unsafe {
        libsodium_sys::crypto_secretbox_open(secret.as_mut_ptr(), secret_resp.secret.as_ptr(), secret_resp.secret.len() as u64, secret_resp.secret_nonce.as_ptr(), secret_key.as_ptr());
    }


    Ok(secret)
}

fn generate_keypair() -> (StaticSecret, PublicKey) {
    let secret_key = StaticSecret::random_from_rng(OsRng);
    let pub_key = PublicKey::from(&secret_key);

    (secret_key, pub_key)
}

const ENC_WG_KEY_LEN: usize = libsodium_sys::crypto_box_SEALBYTES as usize + 32;

pub async fn add_charger(access_token: &String, secret: &[u8], base_url: &String) -> anyhow::Result<State> {
    let self_keypairs: Vec<(StaticSecret, PublicKey)> = (0..5).map(|_| generate_keypair()).collect();
    let server_keypairs: Vec<(StaticSecret, PublicKey)> = (0..5).map(|_| generate_keypair()).collect();
    let psk_vec: Vec<StaticSecret> = (0..5).map(|_| StaticSecret::random_from_rng(OsRng)).collect();

    let mut public = vec![0u8; 32];
    unsafe {
        libsodium_sys::crypto_scalarmult_base(public.as_mut_ptr(), secret.as_ptr());
    }

    let encrypted_psk_vec: Vec<[u8; ENC_WG_KEY_LEN]> = psk_vec.iter().map(|psk| {
        let mut enc = [0u8; ENC_WG_KEY_LEN];
        unsafe {
            libsodium_sys::crypto_box_seal(enc.as_mut_ptr(), psk.as_bytes().as_ptr(), 32, public.as_ptr());
        }
        enc
    }).collect();
    let encrypted_pivate_keys: Vec<[u8; ENC_WG_KEY_LEN]> = server_keypairs.iter().map(|key| {
        let mut enc = [0u8; ENC_WG_KEY_LEN];
        unsafe  {
            libsodium_sys::crypto_box_seal(enc.as_mut_ptr(), key.0.as_bytes().as_ptr(), 32, public.as_ptr());
        }
        enc
    }).collect();

    let local_management_secret = StaticSecret::random_from_rng(OsRng);
    let local_management_pub = PublicKey::from(&local_management_secret);
    let local_management_psk = StaticSecret::random_from_rng(OsRng);
    let engine = base64::engine::general_purpose::STANDARD;
    let keys: Vec<backend::routes::charger::add::Keys> = encrypted_psk_vec.into_iter().enumerate().map(|(i, psk)| {
        backend::routes::charger::add::Keys {
            web_private: encrypted_pivate_keys[i].clone().to_vec(),
            psk: psk.to_vec(),
            charger_address: IpNetwork::V4(Ipv4Network::new(Ipv4Addr::new(123, 123, 123, 2), 24).unwrap()),
            charger_public: engine.encode(self_keypairs[i].1.as_bytes().to_vec()),
            connection_no: i as u16,
            web_address: IpNetwork::V4(Ipv4Network::new(Ipv4Addr::new(123, 123, 123, 3), 24).unwrap()),
        }
    }).collect();

    let charger = ChargerSchema {
        wg_charger_ip: IpNetwork::V4(Ipv4Network::new(Ipv4Addr::new(123, 123, 123, 3), 24).unwrap()),
        wg_server_ip: IpNetwork::V4(Ipv4Network::new(Ipv4Addr::new(123, 123, 123, 4), 24).unwrap()),
        id: bs58::encode(ID.to_be_bytes()).with_alphabet(bs58::Alphabet::FLICKR).into_string(),
        name: "Emulator".to_owned(),
        charger_pub: engine.encode(local_management_pub.as_bytes()),
        psk: engine.encode(local_management_psk.as_bytes()),
    };
    let add_schema = AddChargerSchema {
        charger,
        keys: keys.try_into().unwrap()
    };

    let client = create_http_client()?;
    let resp: AddChargerResponseSchema = client.put(format!("{}/api/charger/add", base_url))
        .body(serde_json::to_string(&add_schema)?)
        .header(reqwest::header::COOKIE, format!("access_token={}", access_token))
        .send()
        .await?
        .json()
        .await?;
    let server_public = engine.decode(resp.management_pub)?;
    let server_public: [u8; 32] = server_public.try_into().unwrap();
    let server_public = PublicKey::from(server_public);

    let state_keypairs = self_keypairs.iter().enumerate().map(|(i, (secret, _))| {
        (secret.to_owned(), server_keypairs[i].1)
    }).collect();

    let ret = State {
        local_management_secret,
        server_management_public: server_public,
        password: resp.charger_password,
        remote_keys: state_keypairs,
    };

    Ok(ret)
}

pub async fn management_discovery(state: &State, base_url: &String) -> anyhow::Result<()> {
    let data = ManagementDataVersion::V1(ManagementDataVersion1 {
        configured_connections: (0..5).map(|i| i).collect(),
        firmware_version: "2.4.0".to_string(),
        port: 80
    });
    let management_schema = ManagementSchema {
        data,
        id: ID,
        password: state.password.clone()
    };

    let client = create_http_client()?;
    let body = serde_json::to_string(&management_schema)?;
    log::info!("{}", body);
    let resp = client.put(format!("{}/api/management", base_url))
        .body(body)
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .send()
        .await?;

    if resp.status() != 200 {
        let resp = resp.text().await?;
        log::error!("Management discovery failed: {}", resp);
    }

    Ok(())
}
