use actix_web::{post, web, HttpResponse, Responder};
use base64::prelude::*;
use db_connector::models::{allowed_users::AllowedUser, chargers::Charger, wg_keys::WgKey};
use diesel::{prelude::*, result::Error::NotFound};
use serde::{Deserialize, Serialize};
use validator::{Validate, ValidationError};

use crate::{error::Error, utils::get_connection, AppState};

#[derive(Serialize, Deserialize, Clone, Default, Validate)]
struct Keys {
    web_private: String,
    charger_public: String,
}

#[derive(Serialize, Deserialize, Clone)]
struct ChargerSchema {
    id: String,
    name: String,
}

// maybe add validator?
#[derive(Serialize, Deserialize, Validate)]
#[validate(schema(function = "validate_add_charger_schema"))]
struct AddChargerSchema {
    charger: ChargerSchema,
    keys: [Keys; 5],
}

fn validate_add_charger_schema(schema: &AddChargerSchema) -> Result<(), ValidationError> {
    for key in schema.keys.iter() {
        validate_wg_key(&key.charger_public)?;
        validate_wg_key(&key.web_private)?;
    }

    Ok(())
}

fn validate_wg_key(key: &str) -> Result<(), ValidationError> {
    let key = match BASE64_STANDARD.decode(key) {
        Ok(key) => key,
        Err(_) => return Err(ValidationError::new("Invalid base64 encoding.")),
    };

    println!("key_len = {}", key.len());
    if key.len() != 32 {
        return Err(ValidationError::new("Data is no valid key"));
    }

    Ok(())
}

#[post("/add")]
pub async fn add(
    state: web::Data<AppState>,
    charger: web::Json<AddChargerSchema>,
    uid: crate::models::uuid::Uuid,
) -> Result<impl Responder, actix_web::Error> {
    add_charger(charger.charger.clone(), uid.clone().into(), &state).await?;

    for keys in charger.keys.iter() {
        add_wg_key(
            charger.charger.id.clone(),
            uid.clone().into(),
            keys.clone(),
            &state,
        )
        .await?;
    }

    Ok(HttpResponse::Ok())
}

async fn add_charger(
    charger: ChargerSchema,
    uid: uuid::Uuid,
    state: &web::Data<AppState>,
) -> Result<(), actix_web::Error> {
    use crate::schema::allowed_users::dsl as allowed_users;
    use crate::schema::chargers::dsl as chargers;

    let mut conn = get_connection(state)?;
    match web::block(move || {
        match chargers::chargers
            .find(&charger.id)
            .select(Charger::as_select())
            .get_result(&mut conn)
        {
            Ok(_) => return Err(Error::ChargerAlreadyExists),
            Err(NotFound) => (),
            Err(_err) => return Err(Error::InternalError),
        }

        let charger = Charger {
            id: charger.id,
            name: charger.name,
            last_ip: None,
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
            user: uid,
            charger: charger.id,
            is_owner: true,
        };

        match diesel::insert_into(allowed_users::allowed_users)
            .values(user)
            .execute(&mut conn)
        {
            Ok(_) => (),
            Err(_err) => return Err(Error::InternalError),
        }

        Ok(())
    })
    .await
    {
        Ok(res) => match res {
            Ok(()) => Ok(()),
            Err(err) => Err(err.into()),
        },
        Err(_err) => Err(Error::InternalError.into()),
    }
}

async fn add_wg_key(
    cid: String,
    uid: uuid::Uuid,
    keys: Keys,
    state: &web::Data<AppState>,
) -> Result<(), actix_web::Error> {
    use crate::schema::wg_keys::dsl::*;
    let mut conn = get_connection(state)?;

    let keys = WgKey {
        id: uuid::Uuid::new_v4(),
        user_id: uid,
        charger: cid,
        in_use: false,
        charger_pub: keys.charger_public,
        user_private: keys.web_private,
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
    use super::*;
    use actix_web::{cookie::Cookie, test, App};
    use boringtun::x25519;
    use rand_core::OsRng;

    use crate::{
        defer,
        middleware::jwt::JwtMiddleware,
        routes::{
            auth::{
                login::tests::verify_and_login_user,
                register::tests::{create_user, delete_user},
            },
            charger::remove::tests::{
                remove_allowed_test_users, remove_test_charger, remove_test_keys,
            },
        },
        tests::configure,
    };

    fn generate_keys() -> [Keys; 5] {
        let mut keys: [Keys; 5] = Default::default();
        for key in keys.iter_mut() {
            let secret = x25519::StaticSecret::random_from_rng(OsRng);
            let public = x25519::PublicKey::from(&secret);
            *key = Keys {
                web_private: BASE64_STANDARD.encode(secret),
                charger_public: BASE64_STANDARD.encode(public),
            }
        }

        keys
    }

    pub async fn add_test_charger(name: &str, token: &str) {
        let app = App::new()
            .configure(configure)
            .wrap(JwtMiddleware)
            .service(add);
        let app = test::init_service(app).await;

        let keys = generate_keys();
        let charger = AddChargerSchema {
            charger: ChargerSchema {
                id: name.to_string(),
                name: name.to_string(),
            },
            keys,
        };

        let req = test::TestRequest::post()
            .uri("/add")
            .cookie(Cookie::new("access_token", token))
            .set_json(charger)
            .to_request();

        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_success());
    }

    #[actix_web::test]
    async fn test_valid_charger() {
        let mail = "valid_charger@test.invalid";
        create_user(mail).await;
        defer!(delete_user(mail));

        let app = App::new()
            .configure(configure)
            .wrap(JwtMiddleware)
            .service(add);
        let app = test::init_service(app).await;

        let keys = generate_keys();
        let cid = "ABC".to_string();
        let charger = AddChargerSchema {
            charger: ChargerSchema {
                id: cid.clone(),
                name: "Test".to_string(),
            },
            keys,
        };

        let token = verify_and_login_user(mail).await;
        let req = test::TestRequest::post()
            .uri("/add")
            .cookie(Cookie::new("access_token", token))
            .set_json(charger)
            .to_request();

        let resp = test::call_service(&app, req).await;
        remove_test_keys(mail);
        remove_allowed_test_users(&cid);
        remove_test_charger(&cid);
        assert!(resp.status().is_success());
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
        let keys = generate_keys();
        let schema = AddChargerSchema {
            charger: ChargerSchema {
                id: "ABC".to_string(),
                name: "Test".to_string(),
            },
            keys,
        };

        assert!(validate_add_charger_schema(&schema).is_ok());
    }
}