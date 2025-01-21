use actix_web::{put, web, HttpResponse, Responder};
use base64::{prelude::BASE64_STANDARD, Engine};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use validator::{Validate, ValidationError};

use crate::{
    routes::charger::add::{register_charger, AddChargerSchema, ChargerSchema},
    utils::{parse_uuid, validate_auth_token},
    AppState,
};

use super::add::Keys;

#[derive(Deserialize, Serialize, ToSchema, Validate)]
#[validate(schema(function = "validate_add_charger_with_token_schema"))]
pub struct AddChargerWithTokenSchema {
    pub token: String,
    pub user_id: String,
    pub charger: ChargerSchema,
    pub keys: [Keys; 5],
    pub name: String,
    pub note: String,
}

fn validate_add_charger_with_token_schema(
    schema: &AddChargerWithTokenSchema,
) -> Result<(), ValidationError> {
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

/// Add a charger using an authorization token instead of JWT authentication
#[utoipa::path(
    request_body = AddChargerWithTokenSchema,
    responses(
        (status = 200, description = "Charger was added successfully", body = AddChargerResponseSchema),
        (status = 401, description = "The authorization token is invalid"),
        (status = 400, description = "The request contains invalid data")
    )
)]
#[put("/add_with_token")]
pub async fn add_with_token(
    state: web::Data<AppState>,
    body: actix_web_validator::Json<AddChargerWithTokenSchema>,
) -> actix_web::Result<impl Responder> {
    let user_id = parse_uuid(&body.user_id)?;

    // Validate the authorization token
    validate_auth_token(body.token.clone(), user_id, &state).await?;

    let schema = AddChargerSchema {
        charger: body.charger.clone(),
        keys: body.keys.clone(),
        name: body.name.clone(),
        note: body.note.clone(),
    };
    let resp = register_charger(state, schema, user_id).await?;

    Ok(HttpResponse::Ok().json(resp))
}

#[cfg(test)]
mod tests {
    use std::net::Ipv4Addr;

    use super::*;
    use crate::{
        routes::{
            charger::{
                add::{tests::generate_random_keys, AddChargerResponseSchema},
                remove::tests::{remove_allowed_test_users, remove_test_charger, remove_test_keys},
            },
            user::tests::{get_test_uuid, TestUser},
        },
        tests::configure,
    };
    use actix_web::{test, App};
    use ipnetwork::{IpNetwork, Ipv4Network};
    use rand::RngCore;
    use rand_core::OsRng;

    #[actix_web::test]
    async fn test_valid_charger() {
        let (mut user, mail) = TestUser::random().await; // store mail
        user.login().await;
        let auth_token = user.create_authorization_token(true).await;

        let app = App::new().configure(configure).service(add_with_token);
        let app = test::init_service(app).await;

        let keys = generate_random_keys();
        let cid = uuid::Uuid::new_v4().to_string();
        let uid = OsRng.next_u32() as i32;
        let charger = AddChargerWithTokenSchema {
            user_id: get_test_uuid(&mail).unwrap().to_string(),
            token: auth_token.token,
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
            .uri("/add_with_token")
            .set_json(charger)
            .to_request();

        let resp = test::call_service(&app, req).await;
        let _ = remove_test_keys(&mail);
        remove_allowed_test_users(&cid);
        remove_test_charger(&cid);
        println!("{:?}", resp);
        println!("{:?}", resp.response().body());
        assert!(resp.status().is_success());

        let body: AddChargerResponseSchema = test::read_body_json(resp).await;
        let user_uuid = get_test_uuid(&mail).unwrap().to_string();
        assert_eq!(body.user_id, user_uuid);
    }

    #[actix_web::test]
    async fn test_invalid_token() {
        let (mut user, mail) = TestUser::random().await; // store mail
        user.login().await;
        let auth_token = "invalid_token".to_string();

        let app = App::new().configure(configure).service(add_with_token);
        let app = test::init_service(app).await;

        let keys = generate_random_keys();
        let cid = uuid::Uuid::new_v4().to_string();
        let uid = OsRng.next_u32() as i32;
        let charger = AddChargerWithTokenSchema {
            user_id: get_test_uuid(&mail).unwrap().to_string(),
            token: auth_token,
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
            .uri("/add_with_token")
            .set_json(charger)
            .to_request();

        let resp = test::call_service(&app, req).await;
        let _ = remove_test_keys(&mail);
        remove_allowed_test_users(&cid);
        remove_test_charger(&cid);
        println!("{:?}", resp);
        println!("{:?}", resp.response().body());
        assert_eq!(resp.status(), 401);
    }
}
