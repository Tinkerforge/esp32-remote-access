/* esp32-remote-access
 * Copyright (C) 2024 Frederic Henrichs <frederic@tinkerforge.com>
 *
 * This library is free software; you can redistribute it and/or
 * modify it under the terms of the GNU Lesser General Public
 * License as published by the Free Software Foundation; either
 * version 2 of the License, or (at your option) any later version.
 *
 * This library is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU
 * Lesser General Public License for more details.
 *
 * You should have received a copy of the GNU Lesser General Public
 * License along with this library; if not, write to the
 * Free Software Foundation, Inc., 59 Temple Place - Suite 330,
 * Boston, MA 02111-1307, USA.
 */

use actix_web::{put, web, HttpResponse, Responder};
use argon2::password_hash::PasswordHashString;
use base64::prelude::*;
use db_connector::models::{allowed_users::AllowedUser, chargers::Charger, wg_keys::WgKey};
use diesel::prelude::*;
use ipnetwork::IpNetwork;
use rand::{distr::Alphanumeric, RngExt};
use rand_core::{OsRng, TryRngCore};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use validator::{Validate, ValidationError};

use crate::{
    error::Error,
    routes::auth::register::hash_key,
    utils::{get_connection, web_block_unpacked},
    AppState,
};

use super::get_charger_uuid;

// z-base-32 alphabet from ZRTP (RFC 6189). Case-insensitive: z-base-32 has no
// characters that are unique to either case, so we always lowercase before
// looking up an index.
const ZBASE32_ALPHABET: &[u8; 32] = b"ybndrfg8ejkmcpqxot1uwisza345h769";

// UIDs above this value are encoded as z-base-32 instead of Flickr-base58.
// Must stay in sync with `ZBASE32_UID_THRESHOLD` in the frontend.
const ZBASE32_UID_THRESHOLD: u32 = 257_899;

/// Decode a z-base-32 string to a u32. Returns `None` if any character is
/// outside the z-base-32 alphabet or if the value overflows u32.
fn zbase32_decode(input: &str) -> Option<u32> {
    let mut value: u32 = 0;
    for byte in input.bytes() {
        let lower = byte.to_ascii_lowercase();
        let index = ZBASE32_ALPHABET.iter().position(|&c| c == lower)? as u32;
        value = value.checked_mul(32)?.checked_add(index)?;
    }
    Some(value)
}

/// Encode a u32 as z-base-32 (RFC 6189 / ZRTP). The do-while loop mirrors
/// the C reference implementation so that 0 encodes to the single
/// character `"y"`.
#[cfg(test)]
fn zbase32_encode(value: u32) -> String {
    let mut result = String::new();
    let mut remaining = value;
    loop {
        let index = (remaining % 32) as usize;
        remaining /= 32;
        result.insert(0, ZBASE32_ALPHABET[index] as char);
        if remaining == 0 {
            break;
        }
    }
    result
}

/// Encode a charger UID using the same scheme as the frontend
/// (`encodeUid` in `base58.ts`): z-base-32 for UIDs above the cutoff,
/// Flickr-base58 for UIDs at or below it.
#[cfg(test)]
pub(crate) fn encode_charger_uid(uid: i32) -> String {
    if uid > ZBASE32_UID_THRESHOLD as i32 {
        zbase32_encode(uid as u32)
    } else {
        bs58::encode(uid.to_be_bytes())
            .with_alphabet(bs58::Alphabet::FLICKR)
            .into_string()
    }
}

/// Decode a charger UID string. The frontend encodes UIDs > 257899 as
/// z-base-32 and smaller UIDs as Flickr-base58; we mirror that here. A
/// string that's a valid z-base-32 representation is also valid base58 (the
/// two alphabets overlap), so we let the decoded value disambiguate:
///
///   - If the input decodes to a value > 257899 in z-base-32, treat it as
///     a z-base-32 UID.
///   - Otherwise (including when the input is not a valid z-base-32
///     string at all), fall back to Flickr-base58.
///
/// Returns `None` if neither decoding succeeds or if the Flickr-base58
/// result does not fit in an i32.
fn decode_charger_uid(uid_str: &str) -> Option<i32> {
    if let Some(value) = zbase32_decode(uid_str) {
        if value > ZBASE32_UID_THRESHOLD {
            return Some(value as i32);
        }
    }

    let mut uid_bytes = bs58::decode(uid_str)
        .with_alphabet(bs58::Alphabet::FLICKR)
        .into_vec()
        .ok()?;
    uid_bytes.reverse();
    let mut device_id_bytes = [0u8; 4];
    for (uid_byte, device_byte) in uid_bytes.into_iter().zip(device_id_bytes.iter_mut()) {
        *device_byte = uid_byte;
    }
    Some(i32::from_le_bytes(device_id_bytes))
}

#[derive(Serialize, Deserialize, Clone, Validate, ToSchema, Debug)]
pub struct Keys {
    #[schema(value_type = Vec<u32>)]
    pub web_private: Vec<u8>,
    #[schema(value_type = Vec<u32>)]
    pub psk: Vec<u8>,
    pub charger_public: String,
    #[schema(value_type = SchemaType::String)]
    pub web_address: IpNetwork,
    #[schema(value_type = SchemaType::String)]
    pub charger_address: IpNetwork,
    pub connection_no: u16,
}

#[derive(Serialize, Deserialize, Clone, ToSchema)]
pub struct ChargerSchema {
    pub uid: String,
    pub charger_pub: String,
    #[schema(value_type = SchemaType::String)]
    pub wg_charger_ip: IpNetwork,
    #[schema(value_type = SchemaType::String)]
    pub wg_server_ip: IpNetwork,
    pub psk: String,
}

#[derive(Serialize, Deserialize, Validate, ToSchema, Clone)]
#[validate(schema(function = "validate_add_charger_schema"))]
pub struct AddChargerSchema {
    pub charger: ChargerSchema,
    pub keys: [Keys; 5],
    pub name: String,
    pub note: String,
}

#[derive(Serialize, Deserialize, ToSchema, Debug)]
pub struct AddChargerResponseSchema {
    pub management_pub: String,
    pub charger_uuid: String,
    pub charger_password: String,
    pub user_id: String,
}

fn validate_add_charger_schema(schema: &AddChargerSchema) -> Result<(), ValidationError> {
    for key in schema.keys.iter() {
        validate_wg_key(&key.charger_public)?;
    }

    validate_wg_key(&schema.charger.charger_pub)?;
    validate_charger_id(&schema.charger.uid)?;

    Ok(())
}

fn validate_wg_key(key: &str) -> Result<(), ValidationError> {
    let key = match BASE64_STANDARD.decode(key) {
        Ok(key) => key,
        Err(_) => return Err(ValidationError::new("Invalid base64 encoding.")),
    };

    if key.len() != 32 {
        return Err(ValidationError::new("Data is no valid key"));
    }

    Ok(())
}

pub(crate) fn validate_charger_id(id: &str) -> Result<(), ValidationError> {
    // Mirror `decode_charger_uid`: any UID above the z-base-32 threshold
    // (including the encoded form itself) is accepted in z-base-32 form.
    // Otherwise it must be a valid Flickr-base58 encoding that fits in
    // 4 bytes.
    if let Some(value) = zbase32_decode(id) {
        if value > ZBASE32_UID_THRESHOLD {
            return Ok(());
        }
    }

    let vec = match bs58::decode(id)
        .with_alphabet(bs58::Alphabet::FLICKR)
        .into_vec()
    {
        Ok(v) => v,
        Err(_) => return Err(ValidationError::new("Data is no valid base58")),
    };

    if vec.len() > 4 {
        return Err(ValidationError::new("Data has wrong length"));
    }

    Ok(())
}

/// Add a new charger.
#[utoipa::path(
    context_path = "/charger",
    request_body = AddChargerSchema,
    responses(
        (status = 200, description = "Adding or updating the charger was successful.", body = AddChargerResponseSchema),
        (status = 401, description = "The charger already exists with another owner"),
    ),
    security(
        ("jwt" = [])
    )
)]
#[put("/add")]
pub async fn add(
    state: web::Data<AppState>,
    device_schema: actix_web_validator::Json<AddChargerSchema>,
    user_id: crate::models::uuid::Uuid,
) -> Result<impl Responder, actix_web::Error> {
    let resp = register_charger(state, device_schema.0, user_id.into()).await?;

    Ok(HttpResponse::Ok().json(resp))
}

pub async fn register_charger(
    state: web::Data<AppState>,
    device_schema: AddChargerSchema,
    user_id: uuid::Uuid,
) -> actix_web::Result<AddChargerResponseSchema> {
    // unwrapping here is safe since it got checked in the validator.
    let device_uid = decode_charger_uid(&device_schema.charger.uid).unwrap();
    let device_id;

    let (pub_key, password) =
        // Updating a charger here is safe since we already had this combination of user and charger
        // and the user_id is not fakable except someone stole our signing key for jwt.
        if let Some(cid) = get_charger_uuid(&state, device_uid, user_id).await? {
            device_id = cid;
            update_charger(
                device_schema.charger.clone(),
                device_id,
                device_uid,
                user_id,
                &state,
            )
            .await?
        } else {
            device_id = uuid::Uuid::new_v4();
            add_charger(
                device_schema.clone(),
                device_id,
                device_uid,
                user_id,
                &state,
            )
            .await?
        };

    for keys in device_schema.keys.iter() {
        add_wg_key(device_id, user_id, keys.to_owned(), &state).await?;
    }

    let user_id: uuid::Uuid = user_id;
    let resp = AddChargerResponseSchema {
        management_pub: pub_key,
        charger_uuid: device_id.to_string(),
        charger_password: password,
        user_id: user_id.to_string(),
    };

    Ok(resp)
}

pub async fn password_matches(
    password: &str,
    password_in_db: &str,
    hasher: &crate::hasher::HasherManager,
) -> actix_web::Result<bool> {
    let password_hash = match PasswordHashString::new(password_in_db) {
        Ok(p) => p,
        Err(_err) => return Err(Error::InternalError.into()),
    };
    let result = hasher
        .verify_password(password_hash, password.as_bytes().to_vec())
        .await;

    Ok(result.is_ok())
}

async fn update_charger(
    device: ChargerSchema,
    device_id: uuid::Uuid,
    device_uid: i32,
    user_id: uuid::Uuid,
    state: &web::Data<AppState>,
) -> actix_web::Result<(String, String)> {
    use db_connector::schema::wg_keys::dsl as wg_keys;

    let mut conn = get_connection(state)?;
    web_block_unpacked(move || {
        if let Err(_err) = diesel::delete(wg_keys::wg_keys)
            .filter(wg_keys::charger_id.eq(device_id))
            .execute(&mut conn)
        {
            return Err(Error::InternalError);
        }

        Ok(())
    })
    .await?;

    let (password, hash) = generate_password(&state.hasher).await?;

    let mut conn = get_connection(state)?;
    web_block_unpacked(move || {
        use db_connector::schema::allowed_users::dsl as allowed_users;

        match diesel::update(
            allowed_users::allowed_users
                .filter(allowed_users::charger_id.eq(device_id))
                .filter(allowed_users::user_id.eq(user_id)),
        )
        .set(allowed_users::valid.eq(true))
        .execute(&mut conn)
        {
            Ok(_) => Ok(()),
            Err(_err) => Err(Error::InternalError),
        }
    })
    .await?;

    let mut conn = get_connection(state)?;
    let pub_key = web_block_unpacked(move || {
        let mut private_key = [0u8; 32];
        if let Err(error) = OsRng.try_fill_bytes(&mut private_key) {
            log::error!("Failed to generate new private key: {error}");
            return Err(Error::InternalError);
        }

        let private_key = boringtun::x25519::StaticSecret::from(private_key);
        let pub_key = boringtun::x25519::PublicKey::from(&private_key);
        let private_key = BASE64_STANDARD.encode(private_key.as_bytes());
        let pub_key = BASE64_STANDARD.encode(pub_key.as_bytes());

        let device = Charger {
            id: device_id,
            uid: device_uid,
            password: hash,
            name: None,
            charger_pub: device.charger_pub,
            management_private: private_key,
            wg_charger_ip: device.wg_charger_ip,
            wg_server_ip: device.wg_server_ip,
            psk: device.psk,
            webinterface_port: 0,
            firmware_version: String::new(),
            last_state_change: Some(chrono::Utc::now().naive_utc()),
            device_type: None,
            mtu: None,
            last_charge_log_upload_hash: Vec::new(),
        };
        match diesel::update(&device).set(&device).execute(&mut conn) {
            Ok(_) => Ok(pub_key),
            Err(_err) => Err(Error::InternalError),
        }
    })
    .await?;

    Ok((pub_key, password))
}

async fn generate_password(
    hasher: &crate::hasher::HasherManager,
) -> actix_web::Result<(String, String)> {
    let password: String = rand::rng()
        .sample_iter(&Alphanumeric)
        .take(32)
        .map(char::from)
        .collect();

    let hash = match hash_key(password.clone().into(), hasher).await {
        Ok(h) => h,
        Err(_err) => return Err(Error::InternalError.into()),
    };

    Ok((password, hash))
}

pub async fn add_charger(
    schema: AddChargerSchema,
    device_id: uuid::Uuid,
    device_uid: i32,
    uid: uuid::Uuid,
    state: &web::Data<AppState>,
) -> Result<(String, String), actix_web::Error> {
    use db_connector::schema::allowed_users::dsl as allowed_users;
    use db_connector::schema::chargers::dsl as chargers;

    let (password, hash) = generate_password(&state.hasher).await?;

    let mut conn = get_connection(state)?;
    let ret = web_block_unpacked(move || {
        let mut private_key = [0u8; 32];
        if let Err(error) = OsRng.try_fill_bytes(&mut private_key) {
            log::error!("Failed to generate new private key: {error}");
            return Err(Error::InternalError);
        }

        let private_key = boringtun::x25519::StaticSecret::from(private_key);
        let pub_key = boringtun::x25519::PublicKey::from(&private_key);
        let private_key = BASE64_STANDARD.encode(private_key.as_bytes());
        let pub_key = BASE64_STANDARD.encode(pub_key.as_bytes());
        let device = &schema.charger;

        let new_device = Charger {
            id: device_id,
            uid: device_uid,
            password: hash,
            name: None,
            charger_pub: device.charger_pub.clone(),
            management_private: private_key,
            wg_charger_ip: device.wg_charger_ip,
            wg_server_ip: device.wg_server_ip,
            psk: device.psk.clone(),
            webinterface_port: 0,
            firmware_version: String::new(),
            last_state_change: None,
            device_type: None,
            mtu: None,
            last_charge_log_upload_hash: Vec::new(),
        };

        match diesel::insert_into(chargers::chargers)
            .values(&new_device)
            .execute(&mut conn)
        {
            Ok(_) => (),
            Err(_err) => return Err(Error::InternalError),
        }

        let user = AllowedUser {
            id: uuid::Uuid::new_v4(),
            user_id: uid,
            charger_id: new_device.id,
            charger_uid: new_device.uid,
            valid: true,
            note: Some(schema.note),
            name: Some(schema.name),
        };

        match diesel::insert_into(allowed_users::allowed_users)
            .values(user)
            .execute(&mut conn)
        {
            Ok(_) => (),
            Err(_err) => return Err(Error::InternalError),
        }

        Ok((pub_key, password))
    })
    .await?;

    Ok(ret)
}

async fn add_wg_key(
    cid: uuid::Uuid,
    uid: uuid::Uuid,
    keys: Keys,
    state: &web::Data<AppState>,
) -> Result<(), actix_web::Error> {
    use db_connector::schema::wg_keys::dsl::*;
    let mut conn = get_connection(state)?;

    let keys = WgKey {
        id: uuid::Uuid::new_v4(),
        user_id: uid,
        charger_id: cid,
        charger_pub: keys.charger_public,
        web_private: keys.web_private,
        psk: keys.psk,
        web_address: keys.web_address,
        charger_address: keys.charger_address,
        connection_no: keys.connection_no as i32,
    };

    match web::block(move || {
        match diesel::insert_into(wg_keys).values(keys).execute(&mut conn) {
            Ok(_) => (),
            Err(_err) => return Err(Error::InternalError),
        }

        Ok(())
    })
    .await
    {
        Ok(res) => match res {
            Ok(()) => (),
            Err(err) => return Err(err.into()),
        },
        Err(_err) => return Err(Error::InternalError.into()),
    }

    Ok(())
}

#[cfg(test)]
pub(crate) mod tests {
    use std::{mem::MaybeUninit, net::Ipv4Addr, str::FromStr};

    use super::*;
    use actix_web::{
        cookie::Cookie,
        test::{self, init_service},
        App,
    };
    use boringtun::x25519;
    use db_connector::test_connection_pool;
    use ipnetwork::Ipv4Network;
    use rand_core::OsRng;

    use crate::{
        middleware::jwt::JwtMiddleware,
        routes::{
            charger::{
                allow_user::UserAuth,
                remove::tests::{remove_allowed_test_users, remove_test_device, remove_test_keys},
                tests::TestCharger,
            },
            user::tests::{get_test_uuid, random_positive_charger_id, TestUser}, // ← add import for UUID check
        },
        tests::configure,
        utils::generate_random_bytes,
    };

    pub fn generate_random_keys() -> [Keys; 5] {
        let mut keys: [MaybeUninit<Keys>; 5] = unsafe { MaybeUninit::uninit().assume_init() };
        for (i, key) in keys.iter_mut().enumerate() {
            let mut private_key = [0u8; 32];
            OsRng.try_fill_bytes(&mut private_key).unwrap();

            let secret = x25519::StaticSecret::from(private_key);
            let public = x25519::PublicKey::from(&secret);
            *key = MaybeUninit::new(Keys {
                web_private: generate_random_bytes(),
                psk: generate_random_bytes(),
                charger_public: BASE64_STANDARD.encode(public),
                charger_address: IpNetwork::V4(
                    Ipv4Network::new("123.123.123.123".parse().unwrap(), 24).unwrap(),
                ),
                web_address: IpNetwork::V4(
                    Ipv4Network::new("123.123.123.122".parse().unwrap(), 24).unwrap(),
                ),
                connection_no: i as u16,
            })
        }

        unsafe { std::mem::transmute::<_, [Keys; 5]>(keys) }
    }

    pub async fn add_test_device(uid: i32, token: &str) -> TestCharger {
        let app = App::new()
            .configure(configure)
            .wrap(JwtMiddleware)
            .service(add);
        let app = test::init_service(app).await;

        println!("Id number: {uid}");
        // Mirror the frontend's `encodeUid`: z-base-32 above the cutoff,
        // Flickr-base58 at or below it. This keeps the round-trip through
        // `decode_charger_uid` deterministic for random UIDs.
        let uid_str = encode_charger_uid(uid);
        println!("id: {uid_str}");
        let keys = generate_random_keys();
        let device = AddChargerSchema {
            charger: ChargerSchema {
                uid: uid_str,
                charger_pub: keys[0].charger_public.clone(),
                wg_charger_ip: IpNetwork::V4(
                    Ipv4Network::new(Ipv4Addr::new(0, 0, 0, 0), 0).unwrap(),
                ),
                wg_server_ip: IpNetwork::V4(
                    Ipv4Network::new(Ipv4Addr::new(0, 0, 0, 0), 0).unwrap(),
                ),
                psk: String::new(),
            },
            keys,
            name: String::new(),
            note: String::new(),
        };

        let req = test::TestRequest::put()
            .uri("/add")
            .cookie(Cookie::new("access_token", token))
            .set_json(device)
            .to_request();

        let resp = test::call_service(&app, req).await;
        let body: AddChargerResponseSchema = test::read_body_json(resp).await;
        TestCharger {
            uid,
            uuid: body.charger_uuid,
            password: body.charger_password,
        }
    }

    #[actix_web::test]
    async fn test_valid_charger() {
        // Use a deterministic UID at or below the cutoff so this test
        // exercises the historical Flickr-base58 encoding path.
        let uid: i32 = 12345;
        let (mut user, mail) = TestUser::random().await; // store mail
        let token = user.login().await;

        let app = App::new()
            .configure(configure)
            .wrap(JwtMiddleware)
            .service(add);
        let app = test::init_service(app).await;

        let keys = generate_random_keys();
        let cid = uuid::Uuid::new_v4().to_string();
        let device = AddChargerSchema {
            charger: ChargerSchema {
                uid: encode_charger_uid(uid),
                charger_pub: keys[0].charger_public.clone(),
                wg_charger_ip: IpNetwork::V4(
                    Ipv4Network::new(Ipv4Addr::new(0, 0, 0, 0), 0).unwrap(),
                ),
                wg_server_ip: IpNetwork::V4(
                    Ipv4Network::new(Ipv4Addr::new(0, 0, 0, 0), 0).unwrap(),
                ),
                psk: String::new(),
            },
            keys,
            name: String::new(),
            note: String::new(),
        };

        let req = test::TestRequest::put()
            .uri("/add")
            .cookie(Cookie::new("access_token", token))
            .set_json(device)
            .to_request();

        let resp = test::call_service(&app, req).await;
        let _ = remove_test_keys(&mail);
        remove_allowed_test_users(&cid);
        remove_test_device(&cid);
        println!("{resp:?}");
        println!("{:?}", resp.response().body());
        assert!(resp.status().is_success());

        let body: AddChargerResponseSchema = test::read_body_json(resp).await;
        let user_uuid = get_test_uuid(&mail).unwrap().to_string();
        assert_eq!(body.user_id, user_uuid);
    }

    #[actix_web::test]
    async fn test_valid_charger_with_zbase32_uid() {
        // Exercise the new z-base-32 path: a UID strictly above the cutoff
        // is sent as z-base-32 (mirroring the frontend's `encodeUid`).
        let uid: i32 = 300_000;
        let (mut user, mail) = TestUser::random().await;
        let token = user.login().await;

        let app = App::new()
            .configure(configure)
            .wrap(JwtMiddleware)
            .service(add);
        let app = test::init_service(app).await;

        let keys = generate_random_keys();
        let cid = uuid::Uuid::new_v4().to_string();
        let uid_str = encode_charger_uid(uid);
        // Sanity-check: the UID > cutoff should have been z-base-32 encoded.
        assert_eq!(zbase32_decode(&uid_str), Some(uid as u32));
        let device = AddChargerSchema {
            charger: ChargerSchema {
                uid: uid_str,
                charger_pub: keys[0].charger_public.clone(),
                wg_charger_ip: IpNetwork::V4(
                    Ipv4Network::new(Ipv4Addr::new(0, 0, 0, 0), 0).unwrap(),
                ),
                wg_server_ip: IpNetwork::V4(
                    Ipv4Network::new(Ipv4Addr::new(0, 0, 0, 0), 0).unwrap(),
                ),
                psk: String::new(),
            },
            keys,
            name: String::new(),
            note: String::new(),
        };

        let req = test::TestRequest::put()
            .uri("/add")
            .cookie(Cookie::new("access_token", token))
            .set_json(device)
            .to_request();

        let resp = test::call_service(&app, req).await;
        let _ = remove_test_keys(&mail);
        remove_allowed_test_users(&cid);
        remove_test_device(&cid);
        println!("{resp:?}");
        println!("{:?}", resp.response().body());
        assert!(resp.status().is_success());

        let body: AddChargerResponseSchema = test::read_body_json(resp).await;
        let user_uuid = get_test_uuid(&mail).unwrap().to_string();
        assert_eq!(body.user_id, user_uuid);
    }

    #[actix_web::test]
    async fn test_update_charger() {
        use db_connector::schema::wg_keys::dsl as wg_keys;
        use diesel::prelude::*;

        let (mut user, mail) = TestUser::random().await; // store mail
        let token = user.login().await.to_owned();
        let device = user.add_random_charger().await;

        let app = App::new()
            .configure(configure)
            .wrap(JwtMiddleware)
            .service(add);
        let app = init_service(app).await;

        let keys = generate_random_keys();
        let device_schema = AddChargerSchema {
            charger: ChargerSchema {
                uid: encode_charger_uid(device.uid),
                charger_pub: keys[0].charger_public.clone(),
                wg_charger_ip: IpNetwork::V4(
                    Ipv4Network::new(Ipv4Addr::new(0, 0, 0, 0), 0).unwrap(),
                ),
                wg_server_ip: IpNetwork::V4(
                    Ipv4Network::new(Ipv4Addr::new(0, 0, 0, 0), 0).unwrap(),
                ),
                psk: String::new(),
            },
            keys,
            name: String::new(),
            note: String::new(),
        };

        let req = test::TestRequest::put()
            .uri("/add")
            .cookie(Cookie::new("access_token", token))
            .set_json(device_schema)
            .to_request();

        let resp = test::call_service(&app, req).await;
        println!("{resp:?}");
        println!("{:?}", resp.response().body());
        assert!(resp.status().is_success());

        let body: AddChargerResponseSchema = test::read_body_json(resp).await;
        let user_uuid = get_test_uuid(&mail).unwrap().to_string();
        assert_eq!(body.user_id, user_uuid);

        let uuid = uuid::Uuid::from_str(&body.charger_uuid).unwrap();
        let pool = test_connection_pool();
        let mut conn = pool.get().unwrap();
        let keys: Vec<WgKey> = wg_keys::wg_keys
            .filter(wg_keys::charger_id.eq(uuid))
            .select(WgKey::as_select())
            .load(&mut conn)
            .unwrap();
        assert_eq!(keys.len(), 5);
    }

    #[actix_web::test]
    async fn test_update_unowned_charger() {
        let (mut user, _) = TestUser::random().await;
        let token = user.login().await.to_owned();
        let mail = user.get_mail().to_owned(); // get mail for UUID check

        let (mut user2, _) = TestUser::random().await;
        user2.login().await;
        let device = user2.add_random_charger().await;
        user2
            .allow_user(
                &mail,
                UserAuth::LoginKey(BASE64_STANDARD.encode(user.get_login_key().await)),
                &device,
            )
            .await;

        let app = App::new()
            .configure(configure)
            .wrap(JwtMiddleware)
            .service(add);
        let app = init_service(app).await;

        let keys = generate_random_keys();
        let device_schema = AddChargerSchema {
            charger: ChargerSchema {
                uid: encode_charger_uid(device.uid),
                charger_pub: keys[0].charger_public.clone(),
                wg_charger_ip: IpNetwork::V4(
                    Ipv4Network::new(Ipv4Addr::new(0, 0, 0, 0), 0).unwrap(),
                ),
                wg_server_ip: IpNetwork::V4(
                    Ipv4Network::new(Ipv4Addr::new(0, 0, 0, 0), 0).unwrap(),
                ),
                psk: String::new(),
            },
            keys,
            name: String::new(),
            note: String::new(),
        };

        let req = test::TestRequest::put()
            .uri("/add")
            .cookie(Cookie::new("access_token", token))
            .set_json(device_schema)
            .to_request();

        let resp = test::call_service(&app, req).await;
        println!("{resp:?}");
        println!("{:?}", resp.response().body());
        assert_eq!(resp.status(), 200);

        let body: AddChargerResponseSchema = test::read_body_json(resp).await;
        let user_uuid = get_test_uuid(&mail).unwrap().to_string();
        assert_eq!(body.user_id, user_uuid);
    }

    #[actix_web::test]
    async fn test_add_existing_charger() {
        let (mut user, _) = TestUser::random().await;
        user.login().await;
        let device = user.add_random_charger().await;

        let (mut user2, _) = TestUser::random().await;
        let user2_mail = user2.get_mail().to_owned(); // store user2 mail
        let token = user2.login().await.to_owned();

        let app = App::new()
            .configure(configure)
            .wrap(JwtMiddleware)
            .service(add);
        let app = init_service(app).await;

        let keys = generate_random_keys();
        let device_schema = AddChargerSchema {
            charger: ChargerSchema {
                uid: encode_charger_uid(device.uid),
                charger_pub: keys[0].charger_public.clone(),
                wg_charger_ip: IpNetwork::V4(
                    Ipv4Network::new(Ipv4Addr::new(0, 0, 0, 0), 0).unwrap(),
                ),
                wg_server_ip: IpNetwork::V4(
                    Ipv4Network::new(Ipv4Addr::new(0, 0, 0, 0), 0).unwrap(),
                ),
                psk: String::new(),
            },
            keys,
            name: String::new(),
            note: String::new(),
        };

        let req = test::TestRequest::put()
            .uri("/add")
            .cookie(Cookie::new("access_token", token))
            .set_json(device_schema)
            .to_request();

        let resp = test::call_service(&app, req).await;
        println!("{resp:?}");
        println!("{:?}", resp.response().body());
        assert_eq!(resp.status().as_u16(), 200);

        let body: AddChargerResponseSchema = test::read_body_json(resp).await;
        let user2_uuid = get_test_uuid(&user2_mail).unwrap().to_string();
        assert_eq!(body.user_id, user2_uuid);
    }

    #[actix_web::test]
    async fn test_key_validator_valid_key() {
        let mut private_key = [0u8; 32];
        OsRng.try_fill_bytes(&mut private_key).unwrap();

        let key = x25519::StaticSecret::from(private_key);
        let key = BASE64_STANDARD.encode(key);
        assert_eq!(Ok(()), validate_wg_key(key.as_str()))
    }

    #[actix_web::test]
    async fn test_key_validator_invalid_key() {
        let mut private_key = [0u8; 32];
        OsRng.try_fill_bytes(&mut private_key).unwrap();
        let key = x25519::StaticSecret::from(private_key);
        let key = BASE64_STANDARD.encode(key);
        assert!(validate_wg_key(&key[0..key.len() - 2]).is_err());

        let key = vec![0u8; 20];
        let key = BASE64_STANDARD.encode(key);
        assert!(validate_wg_key(&key).is_err());

        let key = vec![0u8; 50];
        let key = BASE64_STANDARD.encode(key);
        assert!(validate_wg_key(&key).is_err());
    }

    #[actix_web::test]
    async fn test_validate_add_charger_schema() {
        let keys = generate_random_keys();
        let schema = AddChargerSchema {
            charger: ChargerSchema {
                // Use a value at or below the cutoff so this exercises the
                // historical Flickr-base58 encoding path.
                uid: encode_charger_uid(
                    random_positive_charger_id() % (ZBASE32_UID_THRESHOLD as i32 + 1),
                ),
                charger_pub: keys[0].charger_public.clone(),
                wg_charger_ip: IpNetwork::V4(
                    Ipv4Network::new(Ipv4Addr::new(0, 0, 0, 0), 0).unwrap(),
                ),
                wg_server_ip: IpNetwork::V4(
                    Ipv4Network::new(Ipv4Addr::new(0, 0, 0, 0), 0).unwrap(),
                ),
                psk: String::new(),
            },
            keys,
            name: String::new(),
            note: String::new(),
        };

        assert!(validate_add_charger_schema(&schema).is_ok());
    }

    #[actix_web::test]
    async fn test_validate_add_charger_schema_with_zbase32_uid() {
        let keys = generate_random_keys();
        let schema = AddChargerSchema {
            charger: ChargerSchema {
                // UID strictly above the cutoff -> z-base-32 encoding.
                uid: encode_charger_uid((ZBASE32_UID_THRESHOLD as i32) + 1),
                charger_pub: keys[0].charger_public.clone(),
                wg_charger_ip: IpNetwork::V4(
                    Ipv4Network::new(Ipv4Addr::new(0, 0, 0, 0), 0).unwrap(),
                ),
                wg_server_ip: IpNetwork::V4(
                    Ipv4Network::new(Ipv4Addr::new(0, 0, 0, 0), 0).unwrap(),
                ),
                psk: String::new(),
            },
            keys,
            name: String::new(),
            note: String::new(),
        };

        assert!(validate_add_charger_schema(&schema).is_ok());
    }

    #[test]
    fn zbase32_decode_matches_known_values() {
        // 0
        assert_eq!(zbase32_decode("y"), Some(0));
        // 1
        assert_eq!(zbase32_decode("b"), Some(1));
        // 32 = 1*32 + 0
        assert_eq!(zbase32_decode("by"), Some(32));
        // 32^3 - 1 = 32767 = 31*32^2 + 31*32 + 31
        assert_eq!(zbase32_decode("999"), Some(32_u32.pow(3) - 1));
        // Threshold value (encoded as base58 by the frontend, but the decoder
        // is purely arithmetic and doesn't care about the threshold).
        assert_eq!(zbase32_decode("855m"), Some(257_899));
        // 257_900 = 7*32^3 + 27*32^2 + 27*32 + 12
        assert_eq!(zbase32_decode("855c"), Some(257_900));
        // u32::MAX
        assert_eq!(zbase32_decode("d999999"), Some(u32::MAX));
    }

    #[test]
    fn zbase32_decode_is_case_insensitive() {
        assert_eq!(zbase32_decode("Y"), Some(0));
        assert_eq!(zbase32_decode("B"), Some(1));
        assert_eq!(zbase32_decode("BY"), Some(32));
        assert_eq!(zbase32_decode("d999999"), zbase32_decode("D999999"));
    }

    #[test]
    fn zbase32_decode_rejects_invalid_characters() {
        // '0', 'l', 'v', '2' are not in the z-base-32 alphabet.
        assert_eq!(zbase32_decode("0"), None);
        assert_eq!(zbase32_decode("l"), None);
        assert_eq!(zbase32_decode("v"), None);
        assert_eq!(zbase32_decode("2"), None);
        assert_eq!(zbase32_decode("y0"), None);
        // Characters that aren't ASCII at all are also rejected.
        assert_eq!(zbase32_decode("\u{00e9}"), None);
    }

    #[test]
    fn zbase32_encode_matches_known_values() {
        // The frontend's `intToZBase32` reference values, cross-checked so
        // that `zbase32_encode` and `zbase32_decode` round-trip.
        assert_eq!(zbase32_encode(0), "y");
        assert_eq!(zbase32_encode(1), "b");
        assert_eq!(zbase32_encode(31), "9");
        assert_eq!(zbase32_encode(32), "by");
        assert_eq!(zbase32_encode(257_899), "855m");
        assert_eq!(zbase32_encode(257_900), "855c");
        assert_eq!(zbase32_encode(u32::MAX), "d999999");
    }

    #[test]
    fn zbase32_encode_decode_round_trip() {
        // A spread of values around and above the cutoff.
        for value in [
            0u32,
            1,
            31,
            32,
            33,
            ZBASE32_UID_THRESHOLD - 1,
            ZBASE32_UID_THRESHOLD,
            ZBASE32_UID_THRESHOLD + 1,
            1_000_000,
            u32::MAX - 1,
            u32::MAX,
        ] {
            let encoded = zbase32_encode(value);
            assert_eq!(
                zbase32_decode(&encoded),
                Some(value),
                "z-base-32 round-trip failed for {value}: encoded as {encoded}",
            );
        }
    }

    #[test]
    fn decode_charger_uid_picks_zbase32_above_threshold() {
        // 257_900 is the smallest UID the frontend encodes as z-base-32.
        assert_eq!(decode_charger_uid("855c"), Some(257_900));
        // Larger values round-trip too.
        assert_eq!(decode_charger_uid("d999999"), Some(u32::MAX as i32));
    }

    #[test]
    fn decode_charger_uid_picks_base58_at_or_below_threshold() {
        // 257_899 is the largest UID the frontend encodes as base58, so the
        // canonical encoding is the Flickr-base58 form (using `to_be_bytes` to
        // match the roundtrip the rest of the code performs).
        let base58_257_899 = bs58::encode(257_899_i32.to_be_bytes())
            .with_alphabet(bs58::Alphabet::FLICKR)
            .into_string();
        assert_eq!(decode_charger_uid(&base58_257_899), Some(257_899));
        // The string "855m" decodes to 257_899 in z-base-32, but since that
        // value is not above the threshold we treat it as Flickr-base58,
        // which decodes to a different (still-valid) UID.
        assert_eq!(decode_charger_uid("855m"), Some(1_379_492));
        // UID 100 in base58 (matches what the test fixtures encode).
        let base58_100 = bs58::encode(100_i32.to_be_bytes())
            .with_alphabet(bs58::Alphabet::FLICKR)
            .into_string();
        assert_eq!(decode_charger_uid(&base58_100), Some(100));
    }

    #[test]
    fn decode_charger_uid_rejects_garbage() {
        // '0' is invalid in both alphabets.
        assert_eq!(decode_charger_uid("0"), None);
        // 'l' is invalid in both alphabets.
        assert_eq!(decode_charger_uid("l"), None);
        // Mixed invalid characters are caught even mid-string.
        assert_eq!(decode_charger_uid("by0"), None);
    }

    #[test]
    fn validate_charger_id_accepts_zbase32_above_threshold() {
        // 257_900 in z-base-32 is "855c".
        assert!(validate_charger_id("855c").is_ok());
        // u32::MAX in z-base-32 is "d999999".
        assert!(validate_charger_id("d999999").is_ok());
    }

    #[test]
    fn validate_charger_id_accepts_base58_at_or_below_threshold() {
        // 257_899 in base58.
        let base58_257_899 = bs58::encode(257_899_i32.to_be_bytes())
            .with_alphabet(bs58::Alphabet::FLICKR)
            .into_string();
        assert!(validate_charger_id(&base58_257_899).is_ok());
        // 0 in base58.
        let base58_0 = bs58::encode(0_i32.to_be_bytes())
            .with_alphabet(bs58::Alphabet::FLICKR)
            .into_string();
        assert!(validate_charger_id(&base58_0).is_ok());
    }

    #[test]
    fn validate_charger_id_rejects_invalid_input() {
        // '0' is not in either alphabet.
        assert!(validate_charger_id("0").is_err());
        // 'l' is not in either alphabet.
        assert!(validate_charger_id("l").is_err());
        // Non-ASCII characters are rejected.
        assert!(validate_charger_id("\u{00e9}").is_err());
    }

    #[test]
    fn encode_charger_uid_picks_zbase32_above_threshold() {
        // UIDs strictly greater than the cutoff are encoded as z-base-32.
        assert_eq!(encode_charger_uid(257_900), zbase32_encode(257_900));
        assert_eq!(
            encode_charger_uid(i32::MAX),
            zbase32_encode(i32::MAX as u32),
        );
    }

    #[test]
    fn encode_charger_uid_picks_base58_at_or_below_threshold() {
        // The boundary value and everything below it keeps the historical
        // Flickr-base58 encoding.
        let base58_257_899 = bs58::encode(257_899_i32.to_be_bytes())
            .with_alphabet(bs58::Alphabet::FLICKR)
            .into_string();
        assert_eq!(encode_charger_uid(257_899), base58_257_899);
        let base58_0 = bs58::encode(0_i32.to_be_bytes())
            .with_alphabet(bs58::Alphabet::FLICKR)
            .into_string();
        assert_eq!(encode_charger_uid(0), base58_0);
    }

    #[test]
    fn encode_charger_uid_round_trips_through_decode() {
        // For the base58 branch we need UIDs whose Flickr-base58 form
        // contains a character that's not in the z-base-32 alphabet
        // (case-insensitively) — that is, '0', '2', 'l', or 'v' — to
        // guarantee the round-trip through `decode_charger_uid` is
        // unambiguous. Without that the string is also a valid zbase32
        // string of a much larger value and the decoder picks the wrong
        // branch.
        for uid in [100i32, 101, 257_899] {
            let encoded = encode_charger_uid(uid);
            assert!(
                encoded.chars().any(|c| matches!(c, '0' | '2' | 'l' | 'v' | 'L' | 'V')),
                "test fixture invariant: base58 string for uid {uid} should contain a disambiguating char (0/2/l/v); got {encoded:?}",
            );
            assert_eq!(
                decode_charger_uid(&encoded),
                Some(uid),
                "round-trip failed for uid {uid}: encoded as {encoded}",
            );
        }
        // For the z-base-32 branch the decoder is unambiguous because any
        // string that decodes to a value > the cutoff must be z-base-32.
        for uid in [ZBASE32_UID_THRESHOLD as i32 + 1, 1_000_000, i32::MAX] {
            let encoded = encode_charger_uid(uid);
            assert_eq!(
                zbase32_decode(&encoded),
                Some(uid as u32),
                "test fixture invariant: z-base-32 string for uid {uid} should round-trip; got {encoded:?}",
            );
            assert_eq!(
                decode_charger_uid(&encoded),
                Some(uid),
                "round-trip failed for uid {uid}: encoded as {encoded}",
            );
        }
    }
}
