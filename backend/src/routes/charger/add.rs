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
use rand::{distr::Alphanumeric, Rng, TryRngCore};
use rand_core::OsRng;
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

fn validate_charger_id(id: &str) -> Result<(), ValidationError> {
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
    charger_schema: actix_web_validator::Json<AddChargerSchema>,
    user_id: crate::models::uuid::Uuid,
) -> Result<impl Responder, actix_web::Error> {
    let resp = register_charger(state, charger_schema.0, user_id.into()).await?;

    Ok(HttpResponse::Ok().json(resp))
}

pub async fn register_charger(
    state: web::Data<AppState>,
    charger_schema: AddChargerSchema,
    user_id: uuid::Uuid,
) -> actix_web::Result<AddChargerResponseSchema> {
    // uwrapping here is safe since it got checked in the validator.
    let mut uid_bytes = bs58::decode(&charger_schema.charger.uid)
        .with_alphabet(bs58::Alphabet::FLICKR)
        .into_vec()
        .unwrap();
    uid_bytes.reverse();
    let mut charger_id = [0u8; 4];
    for (i, byte) in uid_bytes.into_iter().enumerate() {
        charger_id[i] = byte;
    }
    let charger_uid = i32::from_le_bytes(charger_id);
    let charger_id;

    let (pub_key, password) =
        // Updating a charger here is safe since we already had this combination of user and charger
        // and the user_id is not fakable except someone stole our signing key for jwt.
        if let Some(cid) = get_charger_uuid(&state, charger_uid, user_id).await? {
            charger_id = cid;
            update_charger(
                charger_schema.charger.clone(),
                charger_id,
                charger_uid,
                user_id,
                &state,
            )
            .await?
        } else {
            charger_id = uuid::Uuid::new_v4();
            add_charger(
                charger_schema.clone(),
                charger_id,
                charger_uid,
                user_id,
                &state,
            )
            .await?
        };

    for keys in charger_schema.keys.iter() {
        add_wg_key(charger_id, user_id, keys.to_owned(), &state).await?;
    }

    let user_id: uuid::Uuid = user_id;
    let resp = AddChargerResponseSchema {
        management_pub: pub_key,
        charger_uuid: charger_id.to_string(),
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
    charger: ChargerSchema,
    charger_id: uuid::Uuid,
    charger_uid: i32,
    user_id: uuid::Uuid,
    state: &web::Data<AppState>,
) -> actix_web::Result<(String, String)> {
    use db_connector::schema::wg_keys::dsl as wg_keys;

    let mut conn = get_connection(state)?;
    web_block_unpacked(move || {
        if let Err(_err) = diesel::delete(wg_keys::wg_keys)
            .filter(wg_keys::charger_id.eq(charger_id))
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
                .filter(allowed_users::charger_id.eq(charger_id))
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

        let charger = Charger {
            id: charger_id,
            uid: charger_uid,
            password: hash,
            name: None,
            charger_pub: charger.charger_pub,
            management_private: private_key,
            wg_charger_ip: charger.wg_charger_ip,
            wg_server_ip: charger.wg_server_ip,
            psk: charger.psk,
            webinterface_port: 0,
            firmware_version: String::new(),
            last_state_change: Some(chrono::Utc::now().naive_utc()),
            device_type: None,
            mtu: None,
        };
        match diesel::update(&charger).set(&charger).execute(&mut conn) {
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
    charger_id: uuid::Uuid,
    charger_uid: i32,
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
        let charger = &schema.charger;

        let charger = Charger {
            id: charger_id,
            uid: charger_uid,
            password: hash,
            name: None,
            charger_pub: charger.charger_pub.clone(),
            management_private: private_key,
            wg_charger_ip: charger.wg_charger_ip,
            wg_server_ip: charger.wg_server_ip,
            psk: charger.psk.clone(),
            webinterface_port: 0,
            firmware_version: String::new(),
            last_state_change: None,
            device_type: None,
            mtu: None,
        };

        match diesel::insert_into(chargers::chargers)
            .values(&charger)
            .execute(&mut conn)
        {
            Ok(_) => (),
            Err(_err) => return Err(Error::InternalError),
        }

        let user = AllowedUser {
            id: uuid::Uuid::new_v4(),
            user_id: uid,
            charger_id: charger.id,
            charger_uid: charger.uid,
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
                remove::tests::{remove_allowed_test_users, remove_test_charger, remove_test_keys},
                tests::TestCharger,
            },
            user::tests::{get_test_uuid, TestUser}, // ← add import for UUID check
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

    pub async fn add_test_charger(uid: i32, token: &str) -> TestCharger {
        let app = App::new()
            .configure(configure)
            .wrap(JwtMiddleware)
            .service(add);
        let app = test::init_service(app).await;

        println!("Id number: {uid}");
        let uid_str = bs58::encode(uid.to_be_bytes())
            .with_alphabet(bs58::Alphabet::FLICKR)
            .into_string();
        println!("id: {uid_str}");
        let keys = generate_random_keys();
        let charger = AddChargerSchema {
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
            .set_json(charger)
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
        let (mut user, mail) = TestUser::random().await; // store mail
        let token = user.login().await;

        let app = App::new()
            .configure(configure)
            .wrap(JwtMiddleware)
            .service(add);
        let app = test::init_service(app).await;

        let keys = generate_random_keys();
        let cid = uuid::Uuid::new_v4().to_string();
        let uid = OsRng.try_next_u32().unwrap() as i32;
        let charger = AddChargerSchema {
            charger: ChargerSchema {
                uid: bs58::encode(uid.to_be_bytes())
                    .with_alphabet(bs58::Alphabet::FLICKR)
                    .into_string(),
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
            .set_json(charger)
            .to_request();

        let resp = test::call_service(&app, req).await;
        let _ = remove_test_keys(&mail);
        remove_allowed_test_users(&cid);
        remove_test_charger(&cid);
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
        let charger = user.add_random_charger().await;

        let app = App::new()
            .configure(configure)
            .wrap(JwtMiddleware)
            .service(add);
        let app = init_service(app).await;

        let keys = generate_random_keys();
        let charger_schema = AddChargerSchema {
            charger: ChargerSchema {
                uid: bs58::encode(charger.uid.to_be_bytes())
                    .with_alphabet(bs58::Alphabet::FLICKR)
                    .into_string(),
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
            .set_json(charger_schema)
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
        let charger = user2.add_random_charger().await;
        user2
            .allow_user(
                &mail,
                UserAuth::LoginKey(BASE64_STANDARD.encode(user.get_login_key().await)),
                &charger,
            )
            .await;

        let app = App::new()
            .configure(configure)
            .wrap(JwtMiddleware)
            .service(add);
        let app = init_service(app).await;

        let keys = generate_random_keys();
        let charger_schema = AddChargerSchema {
            charger: ChargerSchema {
                uid: bs58::encode(charger.uid.to_be_bytes())
                    .with_alphabet(bs58::Alphabet::FLICKR)
                    .into_string(),
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
            .set_json(charger_schema)
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
        let charger = user.add_random_charger().await;

        let (mut user2, _) = TestUser::random().await;
        let user2_mail = user2.get_mail().to_owned(); // store user2 mail
        let token = user2.login().await.to_owned();

        let app = App::new()
            .configure(configure)
            .wrap(JwtMiddleware)
            .service(add);
        let app = init_service(app).await;

        let keys = generate_random_keys();
        let charger_schema = AddChargerSchema {
            charger: ChargerSchema {
                uid: bs58::encode(charger.uid.to_be_bytes())
                    .with_alphabet(bs58::Alphabet::FLICKR)
                    .into_string(),
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
            .set_json(charger_schema)
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
                uid: bs58::encode((OsRng.try_next_u32().unwrap() as i32).to_le_bytes())
                    .with_alphabet(bs58::Alphabet::FLICKR)
                    .into_string(),
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
}
