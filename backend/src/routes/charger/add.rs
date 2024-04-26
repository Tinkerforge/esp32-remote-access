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
use argon2::{Argon2, PasswordHash, PasswordVerifier};
use base64::prelude::*;
use db_connector::models::{allowed_users::AllowedUser, chargers::Charger, wg_keys::WgKey};
use diesel::{prelude::*, result::Error::NotFound};
use ipnetwork::IpNetwork;
use rand::{distributions::Alphanumeric, Rng};
use rand_core::OsRng;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use validator::{Validate, ValidationError};

use crate::{
    error::Error,
    routes::{auth::register::hash_key, charger::charger_belongs_to_user},
    utils::{get_connection, web_block_unpacked},
    AppState,
};

#[derive(Serialize, Deserialize, Clone, Validate, ToSchema)]
pub struct Keys {
    #[schema(value_type = Vec<u32>)]
    web_private: Vec<u8>,
    #[schema(value_type = Vec<u32>)]
    web_private_iv: Vec<u8>,
    charger_public: String,
    #[schema(value_type = SchemaType::String)]
    web_address: IpNetwork,
    #[schema(value_type = SchemaType::String)]
    charger_address: IpNetwork,
    connection_no: u16,
}

#[derive(Serialize, Deserialize, Clone, ToSchema)]
pub struct ChargerSchema {
    id: String,
    name: String,
    charger_pub: String,
    #[schema(value_type = SchemaType::String)]
    wg_charger_ip: IpNetwork,
    #[schema(value_type = SchemaType::String)]
    wg_server_ip: IpNetwork,
}

#[derive(Serialize, Deserialize, Validate, ToSchema)]
#[validate(schema(function = "validate_add_charger_schema"))]
pub struct AddChargerSchema {
    charger: ChargerSchema,
    keys: [Keys; 5],
}

#[derive(Serialize, Deserialize, ToSchema)]
pub struct AddChargerResponseSchema {
    management_pub: String,
    charger_password: String,
}

fn validate_add_charger_schema(schema: &AddChargerSchema) -> Result<(), ValidationError> {
    for key in schema.keys.iter() {
        validate_wg_key(&key.charger_public)?;
    }

    validate_wg_key(&schema.charger.charger_pub)?;
    validate_charger_id(&schema.charger.id)?;

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
    uid: crate::models::uuid::Uuid,
) -> Result<impl Responder, actix_web::Error> {
    // uwrapping here is safe since it got checked in the validator.
    let mut id_bytes = bs58::decode(&charger_schema.charger.id)
        .with_alphabet(bs58::Alphabet::FLICKR)
        .into_vec()
        .unwrap();
    id_bytes.reverse();
    let mut charger_id = [0u8; 4];
    for (i, byte) in id_bytes.into_iter().enumerate() {
        charger_id[i] = byte;
    }
    let charger_id = i32::from_le_bytes(charger_id);

    let (pub_key, password) = if charger_exists(charger_id, &state).await? {
        if charger_belongs_to_user(&state, uid.clone().into(), charger_id).await? {
            update_charger(charger_schema.charger.clone(), charger_id, &state).await?
        } else {
            return Err(Error::UserIsNotOwner.into());
        }
    } else {
        add_charger(
            charger_schema.charger.clone(),
            charger_id,
            uid.clone().into(),
            &state,
        )
        .await?
    };

    for keys in charger_schema.keys.iter() {
        add_wg_key(charger_id, uid.clone().into(), keys.to_owned(), &state).await?;
    }

    let resp = AddChargerResponseSchema {
        management_pub: pub_key,
        charger_password: password,
    };

    Ok(HttpResponse::Ok().json(resp))
}

async fn charger_exists(charger_id: i32, state: &web::Data<AppState>) -> actix_web::Result<bool> {
    use db_connector::schema::chargers::dsl as chargers;

    let mut conn = get_connection(state)?;
    let exists = web_block_unpacked(move || {
        match chargers::chargers
            .find(charger_id)
            .select(Charger::as_select())
            .get_result(&mut conn)
        {
            Ok(_) => Ok(true),
            Err(NotFound) => Ok(false),
            Err(_err) => Err(Error::InternalError),
        }
    })
    .await?;

    Ok(exists)
}

pub async fn get_charger_from_db(
    charger_id: i32,
    state: &web::Data<AppState>,
) -> actix_web::Result<Charger> {
    let mut conn = get_connection(state)?;
    let charger: Charger = web_block_unpacked(move || {
        use db_connector::schema::chargers::dsl::*;

        match chargers
            .filter(id.eq(charger_id))
            .select(Charger::as_select())
            .get_result(&mut conn)
        {
            Ok(c) => Ok(c),
            Err(_err) => {
                return Err(Error::InternalError);
            }
        }
    })
    .await?;

    Ok(charger)
}

pub fn password_matches(password: String, password_in_db: String) -> actix_web::Result<bool> {
    let password_hash = match PasswordHash::new(&password_in_db) {
        Ok(p) => p,
        Err(_err) => return Err(Error::InternalError.into()),
    };
    let result = Argon2::default().verify_password(&password.as_bytes(), &password_hash);

    Ok(result.is_ok())
}

async fn update_charger(
    charger: ChargerSchema,
    charger_id: i32,
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

    let (password, hash) = generate_password().await?;

    let mut conn = get_connection(state)?;
    let pub_key = web_block_unpacked(move || {
        let private_key = boringtun::x25519::StaticSecret::random_from_rng(OsRng);
        let pub_key = boringtun::x25519::PublicKey::from(&private_key);
        let private_key = BASE64_STANDARD.encode(private_key.as_bytes());
        let pub_key = BASE64_STANDARD.encode(pub_key.as_bytes());

        let charger = Charger {
            id: charger_id,
            password: hash,
            name: charger.name,
            last_ip: None,
            charger_pub: charger.charger_pub,
            management_private: private_key,
            wg_charger_ip: charger.wg_charger_ip,
            wg_server_ip: charger.wg_server_ip,
        };
        match diesel::update(&charger).set(&charger).execute(&mut conn) {
            Ok(_) => Ok(pub_key),
            Err(_err) => Err(Error::InternalError),
        }
    })
    .await?;

    Ok((pub_key, password))
}

async fn generate_password() -> actix_web::Result<(String, String)> {
    let password: String = rand::thread_rng()
        .sample_iter(&Alphanumeric)
        .take(32)
        .map(char::from)
        .collect();

    let cpy = password.clone();
    let hash = web_block_unpacked(move || match hash_key(cpy.as_bytes()) {
        Ok(h) => Ok(h),
        Err(_err) => return Err(Error::InternalError.into()),
    })
    .await?;

    Ok((password, hash))
}

async fn add_charger(
    charger: ChargerSchema,
    charger_id: i32,
    uid: uuid::Uuid,
    state: &web::Data<AppState>,
) -> Result<(String, String), actix_web::Error> {
    use db_connector::schema::allowed_users::dsl as allowed_users;
    use db_connector::schema::chargers::dsl as chargers;

    let (password, hash) = generate_password().await?;

    let mut conn = get_connection(state)?;
    let ret = web_block_unpacked(move || {
        let private_key = boringtun::x25519::StaticSecret::random_from_rng(OsRng);
        let pub_key = boringtun::x25519::PublicKey::from(&private_key);
        let private_key = BASE64_STANDARD.encode(private_key.as_bytes());
        let pub_key = BASE64_STANDARD.encode(pub_key.as_bytes());

        let charger = Charger {
            id: charger_id,
            password: hash,
            name: charger.name,
            last_ip: None,
            charger_pub: charger.charger_pub,
            management_private: private_key,
            wg_charger_ip: charger.wg_charger_ip,
            wg_server_ip: charger.wg_server_ip,
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
            is_owner: true,
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
    cid: i32,
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
        in_use: false,
        charger_pub: keys.charger_public,
        web_private: keys.web_private,
        web_private_iv: keys.web_private_iv,
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
    use std::{mem::MaybeUninit, net::Ipv4Addr};

    use super::*;
    use actix_web::{
        cookie::Cookie,
        test::{self, init_service},
        App,
    };
    use boringtun::x25519;
    use db_connector::test_connection_pool;
    use ipnetwork::Ipv4Network;
    use rand::RngCore;
    use rand_core::OsRng;

    use crate::{
        middleware::jwt::JwtMiddleware,
        routes::{
            charger::remove::tests::{
                remove_allowed_test_users, remove_test_charger, remove_test_keys,
            },
            user::tests::TestUser,
        },
        tests::configure,
        utils::generate_random_bytes,
    };

    fn generate_random_keys() -> [Keys; 5] {
        let mut keys: [MaybeUninit<Keys>; 5] = unsafe { MaybeUninit::uninit().assume_init() };
        for key in keys.iter_mut() {
            let secret = x25519::StaticSecret::random_from_rng(OsRng);
            let public = x25519::PublicKey::from(&secret);
            *key = MaybeUninit::new(Keys {
                web_private: generate_random_bytes(),
                web_private_iv: generate_random_bytes(),
                charger_public: BASE64_STANDARD.encode(public),
                charger_address: IpNetwork::V4(
                    Ipv4Network::new("123.123.123.123".parse().unwrap(), 24).unwrap(),
                ),
                web_address: IpNetwork::V4(
                    Ipv4Network::new("123.123.123.122".parse().unwrap(), 24).unwrap(),
                ),
                connection_no: 1234,
            })
        }

        unsafe { std::mem::transmute::<_, [Keys; 5]>(keys) }
    }

    pub async fn add_test_charger(id: i32, token: &str) -> String {
        let app = App::new()
            .configure(configure)
            .wrap(JwtMiddleware)
            .service(add);
        let app = test::init_service(app).await;

        println!("Id number: {}", id);
        let id = bs58::encode(id.to_be_bytes())
            .with_alphabet(bs58::Alphabet::FLICKR)
            .into_string();
        println!("id: {}", id);
        let keys = generate_random_keys();
        let charger = AddChargerSchema {
            charger: ChargerSchema {
                id,
                name: uuid::Uuid::new_v4().to_string(),
                charger_pub: keys[0].charger_public.clone(),
                wg_charger_ip: IpNetwork::V4(
                    Ipv4Network::new(Ipv4Addr::new(0, 0, 0, 0), 0).unwrap(),
                ),
                wg_server_ip: IpNetwork::V4(
                    Ipv4Network::new(Ipv4Addr::new(0, 0, 0, 0), 0).unwrap(),
                ),
            },
            keys,
        };

        let req = test::TestRequest::put()
            .uri("/add")
            .cookie(Cookie::new("access_token", token))
            .set_json(charger)
            .to_request();

        let resp = test::call_service(&app, req).await;
        let body: AddChargerResponseSchema = test::read_body_json(resp).await;

        body.charger_password
    }

    #[actix_web::test]
    async fn test_valid_charger() {
        let (mut user, username) = TestUser::random().await;
        let token = user.login().await;

        let app = App::new()
            .configure(configure)
            .wrap(JwtMiddleware)
            .service(add);
        let app = test::init_service(app).await;

        let keys = generate_random_keys();
        let cid = OsRng.next_u32() as i32;
        let charger = AddChargerSchema {
            charger: ChargerSchema {
                id: bs58::encode(cid.to_be_bytes())
                    .with_alphabet(bs58::Alphabet::FLICKR)
                    .into_string(),
                name: "Test".to_string(),
                charger_pub: keys[0].charger_public.clone(),
                wg_charger_ip: IpNetwork::V4(
                    Ipv4Network::new(Ipv4Addr::new(0, 0, 0, 0), 0).unwrap(),
                ),
                wg_server_ip: IpNetwork::V4(
                    Ipv4Network::new(Ipv4Addr::new(0, 0, 0, 0), 0).unwrap(),
                ),
            },
            keys,
        };

        let req = test::TestRequest::put()
            .uri("/add")
            .cookie(Cookie::new("access_token", token))
            .set_json(charger)
            .to_request();

        let resp = test::call_service(&app, req).await;
        remove_test_keys(&username);
        remove_allowed_test_users(cid);
        remove_test_charger(cid);
        println!("{:?}", resp);
        println!("{:?}", resp.response().body());
        assert!(resp.status().is_success());
    }

    #[actix_web::test]
    async fn test_update_charger() {
        use db_connector::schema::wg_keys::dsl as wg_keys;

        let (mut user, _) = TestUser::random().await;
        let token = user.login().await.to_owned();
        let (charger_id, _) = user.add_random_charger().await;

        let app = App::new()
            .configure(configure)
            .wrap(JwtMiddleware)
            .service(add);
        let app = init_service(app).await;

        let keys = generate_random_keys();
        let charger = AddChargerSchema {
            charger: ChargerSchema {
                id: bs58::encode(charger_id.to_be_bytes())
                    .with_alphabet(bs58::Alphabet::FLICKR)
                    .into_string(),
                name: "Test".to_string(),
                charger_pub: keys[0].charger_public.clone(),
                wg_charger_ip: IpNetwork::V4(
                    Ipv4Network::new(Ipv4Addr::new(0, 0, 0, 0), 0).unwrap(),
                ),
                wg_server_ip: IpNetwork::V4(
                    Ipv4Network::new(Ipv4Addr::new(0, 0, 0, 0), 0).unwrap(),
                ),
            },
            keys,
        };

        let req = test::TestRequest::put()
            .uri("/add")
            .cookie(Cookie::new("access_token", token))
            .set_json(charger)
            .to_request();

        let resp = test::call_service(&app, req).await;
        println!("{:?}", resp);
        println!("{:?}", resp.response().body());
        assert!(resp.status().is_success());

        let pool = test_connection_pool();
        let mut conn = pool.get().unwrap();
        let keys: Vec<WgKey> = wg_keys::wg_keys
            .filter(wg_keys::charger_id.eq(charger_id))
            .load(&mut conn)
            .unwrap();
        assert_eq!(keys.len(), 5);
    }


    #[actix_web::test]
    async fn test_update_unowned_charger() {
        let (mut user, username) = TestUser::random().await;
        let token = user.login().await.to_owned();
        let (mut user2, _) = TestUser::random().await;
        user2.login().await;
        let (charger_id, _) = user2.add_random_charger().await;
        user2.allow_user(&username, charger_id).await;

        let app = App::new()
            .configure(configure)
            .wrap(JwtMiddleware)
            .service(add);
        let app = init_service(app).await;

        let keys = generate_random_keys();
        let charger = AddChargerSchema {
            charger: ChargerSchema {
                id: bs58::encode(charger_id.to_be_bytes())
                    .with_alphabet(bs58::Alphabet::FLICKR)
                    .into_string(),
                name: "Test".to_string(),
                charger_pub: keys[0].charger_public.clone(),
                wg_charger_ip: IpNetwork::V4(
                    Ipv4Network::new(Ipv4Addr::new(0, 0, 0, 0), 0).unwrap(),
                ),
                wg_server_ip: IpNetwork::V4(
                    Ipv4Network::new(Ipv4Addr::new(0, 0, 0, 0), 0).unwrap(),
                ),
            },
            keys,
        };

        let req = test::TestRequest::put()
            .uri("/add")
            .cookie(Cookie::new("access_token", token))
            .set_json(charger)
            .to_request();

        let resp = test::call_service(&app, req).await;
        println!("{:?}", resp);
        println!("{:?}", resp.response().body());
        assert_eq!(resp.status(), 401);
    }

    #[actix_web::test]
    async fn test_add_existing_charger() {
        let (mut user, _) = TestUser::random().await;
        user.login().await;
        let (charger, _) = user.add_random_charger().await;
        let (mut user2, _) = TestUser::random().await;
        let token = user2.login().await.to_owned();

        let app = App::new()
            .configure(configure)
            .wrap(JwtMiddleware)
            .service(add);
        let app = init_service(app).await;

        let keys = generate_random_keys();
        let charger = AddChargerSchema {
            charger: ChargerSchema {
                id: bs58::encode(charger.to_be_bytes())
                    .with_alphabet(bs58::Alphabet::FLICKR)
                    .into_string(),
                name: "Test".to_string(),
                charger_pub: keys[0].charger_public.clone(),
                wg_charger_ip: IpNetwork::V4(
                    Ipv4Network::new(Ipv4Addr::new(0, 0, 0, 0), 0).unwrap(),
                ),
                wg_server_ip: IpNetwork::V4(
                    Ipv4Network::new(Ipv4Addr::new(0, 0, 0, 0), 0).unwrap(),
                ),
            },
            keys,
        };

        let req = test::TestRequest::put()
            .uri("/add")
            .cookie(Cookie::new("access_token", token))
            .set_json(charger)
            .to_request();

        let resp = test::call_service(&app, req).await;
        println!("{:?}", resp);
        println!("{:?}", resp.response().body());
        assert_eq!(resp.status().as_u16(), 401);
    }

    #[actix_web::test]
    async fn test_key_validator_valid_key() {
        let key = x25519::StaticSecret::random_from_rng(OsRng);
        let key = BASE64_STANDARD.encode(key);
        assert_eq!(Ok(()), validate_wg_key(key.as_str()))
    }

    #[actix_web::test]
    async fn test_key_validator_invalid_key() {
        let key = x25519::StaticSecret::random_from_rng(OsRng);
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
                id: bs58::encode((OsRng.next_u32() as i32).to_le_bytes())
                    .with_alphabet(bs58::Alphabet::FLICKR)
                    .into_string(),
                name: "Test".to_string(),
                charger_pub: keys[0].charger_public.clone(),
                wg_charger_ip: IpNetwork::V4(
                    Ipv4Network::new(Ipv4Addr::new(0, 0, 0, 0), 0).unwrap(),
                ),
                wg_server_ip: IpNetwork::V4(
                    Ipv4Network::new(Ipv4Addr::new(0, 0, 0, 0), 0).unwrap(),
                ),
            },
            keys,
        };

        assert!(validate_add_charger_schema(&schema).is_ok());
    }
}
