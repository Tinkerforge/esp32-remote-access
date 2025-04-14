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

use actix_web::{post, web, HttpResponse, Responder};
use actix_web_validator::Json;
use argon2::{
    password_hash::{rand_core::OsRng, SaltString},
    Argon2, PasswordHasher,
};
use askama::Template;
use chrono::Days;
use db_connector::models::{users::User, verification::Verification};
use diesel::{prelude::*, result::Error::NotFound};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use validator::Validate;

use crate::{
    error::Error,
    routes::auth::VERIFICATION_EXPIRATION_DAYS,
    utils::{self, get_connection},
    AppState,
};

#[derive(Template)]
#[template(path = "email_verification_en.html")]
pub struct VerifyEmailENTemplate<'a> {
    pub name: &'a str,
    pub link: &'a str,
}

#[derive(Template)]
#[template(path = "email_verification_de.html")]
pub struct VerifyEmailDETemplate<'a> {
    pub name: &'a str,
    pub link: &'a str,
}

#[derive(Debug, Deserialize, Serialize, Validate, Clone, ToSchema)]
pub struct RegisterSchema {
    #[validate(length(min = 3))]
    pub name: String,
    #[validate(email)]
    pub email: String,
    #[schema(value_type = Vec<u32>)]
    pub login_key: Vec<u8>,
    #[schema(value_type = Vec<u32>)]
    pub login_salt: Vec<u8>,
    #[schema(value_type = Vec<u32>)]
    pub secret: Vec<u8>,
    #[schema(value_type = Vec<u32>)]
    pub secret_nonce: Vec<u8>,
    #[schema(value_type = Vec<u32>)]
    pub secret_salt: Vec<u8>,
}

pub fn hash_key(key: &[u8]) -> Result<String, String> {
    let salt = SaltString::generate(&mut OsRng);
    let hashed_password = match Argon2::default().hash_password(key, &salt) {
        Ok(hash) => hash.to_string(),
        Err(err) => return Err(err.to_string()),
    };

    Ok(hashed_password)
}

// This is shown as unused in vscode since vscode assumes you have tests enabled.
#[allow(unused)]
fn send_verification_mail(
    name: String,
    id: Verification,
    email: String,
    state: web::Data<AppState>,
    lang: String,
) -> Result<(), actix_web::Error> {
    let link = format!(
        "{}/api/auth/verify?id={}",
        state.frontend_url,
        id.id.to_string()
    );

    let (body, subject) = match lang.as_str() {
        "de" | "de-DE" => {
            let template = VerifyEmailDETemplate {
                name: &name,
                link: &link,
            };
            match template.render() {
                Ok(body) => (body, "Email verifizieren"),
                Err(_err) => return Err(Error::InternalError.into()),
            }
        }
        _ => {
            let template = VerifyEmailENTemplate {
                name: &name,
                link: &link,
            };
            match template.render() {
                Ok(body) => (body, "Verify email"),
                Err(_err) => return Err(Error::InternalError.into()),
            }
        }
    };

    utils::send_email(&email, subject, body, &state);

    Ok(())
}

/// Register a new user
#[utoipa::path(
    context_path = "/auth",
    responses(
        (status = 201, description = "Registration was successful"),
        (status = 409, description = "A user with this email already exists")
    )
)]
#[post("/register")]
pub async fn register(
    state: web::Data<AppState>,
    data: Json<RegisterSchema>,
    #[cfg(not(test))] lang: crate::models::lang::Lang,
) -> Result<impl Responder, actix_web::Error> {
    let mut conn = get_connection(&state)?;

    let user_mail = data.email.to_lowercase();
    let mail_cpy = user_mail.clone();

    match web::block(move || {
        use db_connector::schema::users::dsl::*;

        match users
            .filter(email.eq(mail_cpy))
            .select(User::as_select())
            .get_result(&mut conn)
        {
            Ok(_) => return Err(Error::UserAlreadyExists),
            Err(NotFound) => (),
            Err(_err) => return Err(Error::InternalError),
        }

        Ok(())
    })
    .await
    {
        Ok(Ok(_)) => (),
        Ok(Err(Error::UserAlreadyExists)) => return Ok(HttpResponse::Created()),
        Ok(Err(_)) => return Err(Error::InternalError.into()),
        Err(_) => return Err(Error::InternalError.into()),
    }

    let key_hash = match hash_key(&data.login_key) {
        Ok(hash) => hash,
        Err(_) => return Err(Error::InternalError.into()),
    };

    let user_insert = User {
        id: uuid::Uuid::new_v4(),
        name: data.name.clone(),
        login_key: key_hash,
        email: user_mail,
        email_verified: false,
        secret: data.secret.clone(),
        secret_nonce: data.secret_nonce.clone(),
        login_salt: data.login_salt.clone(),
        secret_salt: data.secret_salt.clone(),
        delivery_email: Some(data.email.clone()),
        old_delivery_email: None,
        old_email: None,
    };

    let mut conn = get_connection(&state)?;

    let exp = if let Some(expiration) =
        chrono::Utc::now().checked_add_days(Days::new(VERIFICATION_EXPIRATION_DAYS))
    {
        expiration.naive_utc()
    } else {
        return Err(Error::InternalError.into());
    };

    let insert_result = match web::block(move || {
        use db_connector::schema::users::dsl::*;
        use db_connector::schema::verification::dsl::*;

        let verify = Verification {
            id: uuid::Uuid::new_v4(),
            user: user_insert.id.clone(),
            expiration: exp,
        };

        // same as above
        #[allow(unused)]
        let mail = user_insert.email.clone();

        let user_insert_result = diesel::insert_into(users)
            .values(&user_insert)
            .execute(&mut conn);
        match diesel::insert_into(verification)
            .values(&verify)
            .execute(&mut conn)
        {
            Ok(_) => (),
            Err(err) => return Err(err),
        }

        // maybe add mechanism to automatically retry?
        #[cfg(not(test))]
        std::thread::spawn(move || {
            send_verification_mail(
                user_insert.name,
                verify,
                data.email.clone(),
                state.clone(),
                lang.into(),
            )
            .ok();
        });

        user_insert_result
    })
    .await
    {
        Ok(result) => result,
        Err(_err) => return Err(Error::InternalError.into()),
    };

    match insert_result {
        Ok(_) => (),
        Err(_err) => return Err(Error::InternalError.into()),
    }

    Ok(HttpResponse::Created())
}

#[cfg(test)]
pub(crate) mod tests {
    use crate::{defer, tests::configure, utils::generate_random_bytes};
    use actix_web::{http::header::ContentType, test, App};
    use db_connector::test_connection_pool;
    use rand::Rng;

    use super::*;

    pub async fn create_user(mail: &str) -> Vec<u8> {
        // test with valid syntax

        let mut rng = rand::rng();
        let login_key: Vec<u8> = (0..24).map(|_| rng.random_range(0..255)).collect();
        let app = App::new().configure(configure).service(register);
        let app = test::init_service(app).await;
        let user = RegisterSchema {
            name: mail.to_string(),
            email: mail.to_string(),
            login_key: login_key.clone(),
            login_salt: (0..24).map(|_| rng.random_range(0..255)).collect(),
            secret: (0..24).map(|_| rng.random_range(0..255)).collect(),
            secret_nonce: (0..16).map(|_| rng.random_range(0..255)).collect(),
            secret_salt: (0..24).map(|_| rng.random_range(0..255)).collect(),
        };
        let req = test::TestRequest::post()
            .uri("/register")
            .insert_header(ContentType::json())
            .set_json(user)
            .to_request();
        let resp = test::call_service(&app, req).await;
        println!("{}", resp.status());
        assert!(resp.status().is_success());
        println!("Created user");

        login_key
    }

    pub fn delete_user(mail: &str) {
        use db_connector::schema::allowed_users::dsl as allowed_users;
        use db_connector::schema::refresh_tokens::dsl::*;
        use db_connector::schema::users::dsl::*;
        use db_connector::schema::verification::dsl::*;
        use db_connector::schema::wg_keys::dsl as wg_keys;
        use diesel::prelude::*;

        let pool = db_connector::test_connection_pool();
        let mut conn = pool.get().unwrap();
        let mail = mail.to_lowercase();
        let u: User = if let Ok(u) = users
            .filter(email.eq(mail.clone()))
            .select(User::as_select())
            .get_result(&mut conn)
        {
            u
        } else {
            return;
        };

        diesel::delete(refresh_tokens.filter(user_id.eq(u.id)))
            .execute(&mut conn)
            .expect("Error deleting sessions");
        diesel::delete(verification.filter(user.eq(u.id)))
            .execute(&mut conn)
            .expect("Error deleting verification");
        diesel::delete(wg_keys::wg_keys.filter(wg_keys::user_id.eq(u.id)))
            .execute(&mut conn)
            .expect("Error deleting wg keys");
        diesel::delete(allowed_users::allowed_users.filter(allowed_users::user_id.eq(u.id)))
            .execute(&mut conn)
            .expect("Error deleting allowed user object");
        diesel::delete(users.filter(email.eq(mail.to_lowercase())))
            .execute(&mut conn)
            .expect("Error deleting test user");
    }

    fn user_exists(mail: &str) -> bool {
        use db_connector::schema::users::dsl::*;
        use diesel::prelude::*;

        let pool = db_connector::test_connection_pool();
        match users
            .filter(email.eq(mail))
            .select(User::as_select())
            .get_result(&mut pool.get().unwrap())
        {
            Ok(_) => true,
            Err(NotFound) => false,
            Err(err) => panic!("Something went wrong: {}", err),
        }
    }

    #[actix_web::test]
    async fn test_no_data() {
        // Test without data
        let app = App::new().configure(configure).service(register);
        let app = test::init_service(app).await;
        let req = test::TestRequest::post()
            .uri("/register")
            .insert_header(ContentType::json())
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_client_error());
    }

    #[actix_web::test]
    async fn test_invalid_email() {
        // test with invalid email
        let app = App::new().configure(configure).service(register);
        let app = test::init_service(app).await;

        let mail = "Testtest.de";
        let user = RegisterSchema {
            name: "Test".to_string(),
            email: mail.to_string(),
            login_key: generate_random_bytes(),
            login_salt: generate_random_bytes(),
            secret: generate_random_bytes(),
            secret_nonce: generate_random_bytes(),
            secret_salt: generate_random_bytes(),
        };
        let req = test::TestRequest::post()
            .uri("/register")
            .insert_header(ContentType::json())
            .set_json(user)
            .to_request();
        let resp = test::call_service(&app, req).await;

        assert!(resp.status().is_client_error());
        assert_eq!(false, user_exists(mail));
    }

    #[actix_web::test]
    async fn test_short_username() {
        // test with too short username
        let app = App::new().configure(configure).service(register);
        let app = test::init_service(app).await;

        let mail = "Test@test.invalid";
        let user = RegisterSchema {
            name: "Te".to_string(),
            email: mail.to_string(),
            login_key: generate_random_bytes(),
            login_salt: generate_random_bytes(),
            secret: generate_random_bytes(),
            secret_nonce: generate_random_bytes(),
            secret_salt: generate_random_bytes(),
        };
        let req = test::TestRequest::post()
            .uri("/register")
            .insert_header(ContentType::json())
            .set_json(user)
            .to_request();
        let resp = test::call_service(&app, req).await;

        assert!(resp.status().is_client_error());
        assert_eq!(false, user_exists(mail));
    }

    #[actix_web::test]
    async fn test_valid_request() {
        // test with valid syntax
        let app = App::new().configure(configure).service(register);
        let app = test::init_service(app).await;
        let mail = "valid_request@test.invalid";
        let user = RegisterSchema {
            name: "Test".to_string(),
            email: mail.to_string(),
            login_key: generate_random_bytes(),
            login_salt: generate_random_bytes(),
            secret: generate_random_bytes(),
            secret_nonce: generate_random_bytes(),
            secret_salt: generate_random_bytes(),
        };
        let req = test::TestRequest::post()
            .uri("/register")
            .insert_header(ContentType::json())
            .set_json(user)
            .to_request();
        let resp = test::call_service(&app, req).await;
        println!("{}", resp.status());

        assert!(resp.status().is_success());
        assert_eq!(true, user_exists(mail));
        delete_user("valid_request@test.invalid");
    }

    #[actix_web::test]
    async fn test_existing_user() {
        let app = App::new().configure(configure).service(register);
        let app = test::init_service(app).await;
        let mail = "existing_user@test.invalid".to_string();
        let user = RegisterSchema {
            name: "Test".to_string(),
            email: mail.to_string(),
            login_key: generate_random_bytes(),
            login_salt: generate_random_bytes(),
            secret: generate_random_bytes(),
            secret_nonce: generate_random_bytes(),
            secret_salt: generate_random_bytes(),
        };
        let req = test::TestRequest::post()
            .uri("/register")
            .insert_header(ContentType::json())
            .set_json(user.clone())
            .to_request();
        let resp = test::call_service(&app, req).await;
        println!("{}", resp.status());
        assert!(resp.status().is_success());
        defer!(delete_user(mail.as_str()));

        let req = test::TestRequest::post()
            .uri("/register")
            .insert_header(ContentType::json())
            .set_json(user)
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_success());

        let pool = test_connection_pool();
        let mut conn = pool.get().unwrap();
        {
            use db_connector::schema::users::dsl::*;
            use diesel::prelude::*;

            let mail = mail.to_lowercase();
            if let Ok(u) = users
                .filter(email.eq(mail.clone()))
                .select(User::as_select())
                .load(&mut conn)
            {
                if u.len() > 1 {
                    panic!("User was created twice");
                }
            } else {
                panic!("User not found");
            };
        }
    }
}
