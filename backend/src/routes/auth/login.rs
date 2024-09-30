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

use actix_web::{cookie::Cookie, post, web, HttpResponse, Responder};
use actix_web_validator::Json;
use argon2::{Argon2, PasswordHash, PasswordVerifier};
use chrono::{Duration, Utc};
use db_connector::models::{refresh_tokens::RefreshToken, users::User};
use diesel::{
    prelude::*,
    r2d2::{ConnectionManager, PooledConnection},
    result::Error::NotFound,
};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use validator::Validate;

use crate::{
    error::Error,
    models::token_claims::TokenClaims,
    utils::{get_connection, web_block_unpacked},
    AppState,
};

pub const MAX_TOKEN_AGE_MINUTES: i64 = 6;
const MAX_REFRESH_TOKEN_AGE_DAYS: i64 = 7;

#[derive(Serialize, Deserialize, Clone, Debug, Validate, ToSchema)]
pub struct LoginSchema {
    pub email: String,
    #[schema(value_type = Vec<u32>)]
    pub login_key: Vec<u8>,
}

pub enum FindBy {
    Uuid(uuid::Uuid),
    Email(String),
    Username(String),
}

pub async fn validate_password(
    pass: &[u8],
    identifier: FindBy,
    mut conn: PooledConnection<ConnectionManager<PgConnection>>,
) -> Result<uuid::Uuid, actix_web::Error> {
    use db_connector::schema::users::dsl::*;

    let result = web_block_unpacked(move || match identifier {
        FindBy::Email(mail) => Ok(users
            .filter(email.eq(mail))
            .select(User::as_select())
            .get_result(&mut conn)),
        FindBy::Uuid(uid) => Ok(users
            .find(uid)
            .select(User::as_select())
            .get_result(&mut conn)),
        FindBy::Username(username) => Ok(users
            .filter(name.eq(username))
            .select(User::as_select())
            .get_result(&mut conn)),
    })
    .await?;

    let user: User = match result {
        Ok(data) => data,
        Err(NotFound) => return Err(Error::WrongCredentials.into()),
        Err(_err) => return Err(Error::InternalError.into()),
    };

    if !user.email_verified {
        return Err(Error::NotVerified.into());
    }

    let password_hash = match PasswordHash::new(&user.login_key) {
        Ok(hash) => hash,
        Err(_err) => return Err(Error::InternalError.into()),
    };

    match Argon2::default().verify_password(pass, &password_hash) {
        Ok(_) => Ok(user.id),
        Err(_err) => Err(Error::WrongCredentials.into()),
    }
}

/// Login user
#[utoipa::path(
    context_path = "/auth",
    request_body = LoginSchema,
    responses(
        (status = 200, description = "Login was successful"),
        (status = 400, description = "Credentials were incorrect")
    )
)]
#[post("/login")]
pub async fn login(
    state: web::Data<AppState>,
    data: Json<LoginSchema>,
) -> Result<impl Responder, actix_web::Error> {
    let conn = match state.pool.get() {
        Ok(conn) => conn,
        Err(_err) => return Err(Error::InternalError.into()),
    };

    let email = data.email.to_lowercase();
    let uuid = validate_password(&data.login_key, FindBy::Email(email), conn).await?;

    let now = Utc::now();
    let iat = now.timestamp() as usize;
    let exp = (now + Duration::minutes(MAX_TOKEN_AGE_MINUTES)).timestamp() as usize;
    let claims = TokenClaims {
        iat,
        exp,
        sub: uuid.to_string(),
    };

    let token = match jsonwebtoken::encode(
        &jsonwebtoken::Header::default(),
        &claims,
        &jsonwebtoken::EncodingKey::from_secret(state.jwt_secret.as_ref()),
    ) {
        Ok(token) => token,
        Err(_err) => return Err(Error::InternalError.into()),
    };

    let cookie = Cookie::build("access_token", token)
        .path("/")
        .max_age(actix_web::cookie::time::Duration::minutes(
            MAX_TOKEN_AGE_MINUTES,
        ))
        .http_only(true)
        .same_site(actix_web::cookie::SameSite::Strict)
        .secure(true)
        .finish();

    let cookie_string = format!("{}; Partitioned;", cookie.to_string());
    let refresh_cookie = create_refresh_token(&state, uuid).await?;

    Ok(HttpResponse::Ok()
        .append_header(("Set-Cookie", cookie_string))
        .append_header(("Set-Cookie", refresh_cookie))
        .body(""))
}

pub async fn create_refresh_token(
    state: &web::Data<AppState>,
    uid: uuid::Uuid,
) -> actix_web::Result<String> {
    let session_id = uuid::Uuid::new_v4();
    let mut conn = get_connection(state)?;

    let now = Utc::now();
    let iat = now.timestamp() as usize;
    let exp = (now + Duration::days(MAX_REFRESH_TOKEN_AGE_DAYS)).timestamp() as usize;
    let claims = TokenClaims {
        iat,
        exp,
        sub: session_id.to_string(),
    };
    web_block_unpacked(move || {
        use db_connector::schema::refresh_tokens::dsl::*;

        let token = RefreshToken {
            id: session_id,
            user_id: uid,
            expiration: exp as i64,
        };
        match diesel::insert_into(refresh_tokens)
            .values(&token)
            .execute(&mut conn)
        {
            Ok(_) => Ok(()),
            Err(_err) => Err(Error::InternalError),
        }
    })
    .await?;

    let token = match jsonwebtoken::encode(
        &jsonwebtoken::Header::default(),
        &claims,
        &jsonwebtoken::EncodingKey::from_secret(state.jwt_secret.as_ref()),
    ) {
        Ok(token) => token,
        Err(_err) => return Err(Error::InternalError.into()),
    };

    let cookie = Cookie::build("refresh_token", token)
        .path("/")
        .max_age(actix_web::cookie::time::Duration::days(
            MAX_REFRESH_TOKEN_AGE_DAYS,
        ))
        .http_only(true)
        .same_site(actix_web::cookie::SameSite::Strict)
        .secure(true)
        .finish();

    Ok(format!("{}; Partitioned;", cookie.to_string()))
}

#[cfg(test)]
pub(crate) mod tests {
    use actix_web::{http::header::ContentType, test, App};

    use super::*;
    use crate::defer;
    use crate::{
        routes::auth::{
            register::tests::{create_user, delete_user},
            verify::tests::fast_verify,
        },
        tests::configure,
    };

    pub async fn login_user(email: &str, login_key: Vec<u8>) -> (String, String) {
        let app = App::new().configure(configure).service(login);
        let app = test::init_service(app).await;

        let login_schema = LoginSchema {
            email: email.to_string(),
            login_key,
        };
        let req = test::TestRequest::post()
            .uri("/login")
            .insert_header(ContentType::json())
            .set_json(login_schema)
            .to_request();
        let resp = test::call_service(&app, req).await;

        println!("Resp in login_user: {}", resp.status());
        println!("{:?}", resp.response().body());
        assert!(resp.status().is_success());

        let cookies = resp.response().cookies();
        let mut ret = (String::new(), String::new());
        let mut bitmap = 0;
        for cookie in cookies {
            if cookie.name() == "access_token" {
                bitmap |= 1;
                ret.0 = cookie.value().to_owned();
            }
            if cookie.name() == "refresh_token" {
                bitmap |= 2;
                ret.1 = cookie.value().to_owned();
            }
        }
        assert_eq!(bitmap, 3);

        ret
    }

    pub async fn verify_and_login_user(email: &str, login_key: Vec<u8>) -> (String, String) {
        fast_verify(email);

        login_user(email, login_key).await
    }

    #[actix_web::test]
    async fn test_valid_login() {
        let mail = "login@test.invalid";
        let key = create_user(mail).await;
        defer!(delete_user(mail));
        fast_verify(mail);

        let app = App::new().configure(configure).service(login);
        let app = test::init_service(app).await;
        let login_schema = LoginSchema {
            email: mail.to_string(),
            login_key: key,
        };

        let req = test::TestRequest::post()
            .uri("/login")
            .insert_header(ContentType::json())
            .set_json(login_schema)
            .to_request();
        let resp = test::call_service(&app, req).await;

        assert!(resp.status().is_success());

        let cookies = resp.response().cookies();
        let mut bitmap = 0;
        for cookie in cookies {
            if cookie.name() == "access_token" {
                bitmap |= 1;
            }
            if cookie.name() == "refresh_token" {
                bitmap |= 2;
            }
        }
        assert_eq!(bitmap, 3);
    }

    #[actix_web::test]
    async fn test_unverified() {
        let mail = "unverified_login@test.invalid";
        let key = create_user(mail).await;
        defer!(delete_user(mail));

        let app = App::new().configure(configure).service(login);
        let app = test::init_service(app).await;
        let login_schema = LoginSchema {
            email: mail.to_string(),
            login_key: key,
        };

        let req = test::TestRequest::post()
            .uri("/login")
            .insert_header(ContentType::json())
            .set_json(login_schema)
            .to_request();
        let resp = test::call_service(&app, req).await;

        assert!(resp.status().is_client_error());

        let body = test::read_body(resp).await;
        let body = std::str::from_utf8(&body).unwrap();
        assert_eq!(body, "Not verified");
    }
}
