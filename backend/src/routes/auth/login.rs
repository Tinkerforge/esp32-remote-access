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

use actix_web::{cookie::Cookie, post, web, HttpRequest, HttpResponse, Responder};
use actix_web_validator::Json;
use argon2::password_hash::PasswordHashString;
use chrono::{Days, TimeDelta, Utc};
use db_connector::models::{refresh_tokens::RefreshToken, users::User};
use diesel::{result::Error::NotFound, ExpressionMethods, QueryDsl, SelectableHelper};
use diesel_async::RunQueryDsl as _;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use validator::Validate;

use crate::{
    error::Error, models::token_claims::TokenClaims, rate_limit::LoginRateLimiter,
    utils::get_connection, AppState,
};

pub const MAX_TOKEN_AGE_MINUTES: i64 = 6;
const MAX_REFRESH_TOKEN_AGE_DAYS: i64 = 60;

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

/// Look the user up using `identifier` and verify `pass` matches their stored
/// hash. The DB lookup is performed with a freshly-acquired connection that
/// is dropped before we await the hasher — on `actix-web`'s current-thread
/// tokio runtime we must not hold a connection across an await on the
/// `HasherManager` mpsc, otherwise a backlog of registrations / logins can
/// stall the actor entirely.
pub async fn validate_password(
    pass: &[u8],
    identifier: FindBy,
    state: &web::Data<crate::AppState>,
) -> Result<uuid::Uuid, actix_web::Error> {
    use db_connector::schema::users::dsl::*;

    let user: User = {
        let mut conn = crate::utils::get_connection(state).await?;
        match identifier {
            FindBy::Email(mail) => {
                users
                    .filter(email.eq(mail))
                    .select(User::as_select())
                    .get_result::<User>(&mut conn)
                    .await
            }
            FindBy::Uuid(uid) => {
                users
                    .find(uid)
                    .select(User::as_select())
                    .get_result::<User>(&mut conn)
                    .await
            }
            FindBy::Username(username) => {
                users
                    .filter(name.eq(username))
                    .select(User::as_select())
                    .get_result::<User>(&mut conn)
                    .await
            }
        }
    }
    .map_err(|err| match err {
        NotFound => Error::WrongCredentials,
        _ => Error::InternalError,
    })?;

    if !user.email_verified {
        return Err(Error::NotVerified.into());
    }

    let password_hash = match PasswordHashString::new(&user.login_key) {
        Ok(hash) => hash,
        Err(_err) => return Err(Error::InternalError.into()),
    };

    match state
        .hasher
        .verify_password(password_hash, pass.to_vec())
        .await
    {
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
        (status = 401, description = "Credentials were incorrect"),
        (status = 403, description = "Not verified"),
    )
)]
#[post("/login")]
pub async fn login(
    state: web::Data<AppState>,
    data: Json<LoginSchema>,
    rate_limiter: web::Data<LoginRateLimiter>,
    req: HttpRequest,
) -> Result<impl Responder, actix_web::Error> {
    let email = data.email.to_lowercase();
    rate_limiter.check(email.clone(), &req)?;

    let uuid = validate_password(&data.login_key, FindBy::Email(email), &state).await?;

    let now = Utc::now();
    let iat = now.timestamp() as usize;
    let exp = if let Some(exp) =
        now.checked_add_signed(TimeDelta::minutes(super::login::MAX_TOKEN_AGE_MINUTES))
    {
        exp.timestamp() as usize
    } else {
        return Err(Error::InternalError.into());
    };
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

    let cookie_string = format!("{cookie}; Partitioned;");
    let refresh_cookie = create_refresh_token(&state, uuid).await?;

    Ok(HttpResponse::Ok()
        .append_header(("Set-Cookie", cookie_string))
        .append_header(("Set-Cookie", refresh_cookie))
        .body(""))
}

pub async fn create_refresh_token(
    state: &web::Data<AppState>,
    user_id: uuid::Uuid,
) -> actix_web::Result<String> {
    let token_id = uuid::Uuid::new_v4();

    let now = Utc::now();
    let iat = now.timestamp() as usize;
    let exp = if let Some(exp) = now.checked_add_days(Days::new(MAX_REFRESH_TOKEN_AGE_DAYS as u64))
    {
        exp.timestamp() as usize
    } else {
        return Err(Error::InternalError.into());
    };
    let claims = TokenClaims {
        iat,
        exp,
        sub: token_id.to_string(),
    };

    // Insert the refresh token and release the connection before doing the
    // JWT signing work, which doesn't need a database handle.
    {
        let mut conn = get_connection(state).await?;
        use db_connector::schema::refresh_tokens::dsl as refresh_tokens;

        let token = RefreshToken {
            id: token_id,
            user_id,
            expiration: exp as i64,
        };
        match diesel::insert_into(refresh_tokens::refresh_tokens)
            .values(&token)
            .execute(&mut conn)
            .await
        {
            Ok(_) => {}
            Err(_err) => return Err(Error::InternalError.into()),
        }
    }

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

    Ok(format!("{cookie}; Partitioned;"))
}

#[cfg(test)]
pub(crate) mod tests {
    use actix_web::{http::header::ContentType, test, App};

    use super::*;
    use crate::defer_async;
    use crate::{
        routes::auth::{
            register::tests::{create_user, delete_user},
            verify::tests::fast_verify,
        },
        tests::configure,
    };

    pub async fn login_user(email: &str, login_key: Vec<u8>) -> (String, String) {
        println!("Logging in user: {}", email);
        let app = App::new().configure(configure).service(login);
        let app = test::init_service(app).await;

        let login_schema = LoginSchema {
            email: email.to_string(),
            login_key,
        };
        let req = test::TestRequest::post()
            .uri("/login")
            .insert_header(ContentType::json())
            .insert_header(("X-Forwarded-For", "123.123.123.2"))
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
        fast_verify(email).await;

        login_user(email, login_key).await
    }

    #[actix_web::test]
    async fn test_valid_login() {
        let mail = "login@test.invalid";
        let key = create_user(mail).await;
        defer_async!(delete_user(mail));
        fast_verify(mail).await;

        let app = App::new().configure(configure).service(login);
        let app = test::init_service(app).await;
        let login_schema = LoginSchema {
            email: mail.to_string(),
            login_key: key,
        };

        let req = test::TestRequest::post()
            .uri("/login")
            .insert_header(ContentType::json())
            .insert_header(("X-Forwarded-For", "123.123.123.2"))
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
        defer_async!(delete_user(mail));

        let app = App::new().configure(configure).service(login);
        let app = test::init_service(app).await;
        let login_schema = LoginSchema {
            email: mail.to_string(),
            login_key: key,
        };

        let req = test::TestRequest::post()
            .uri("/login")
            .insert_header(ContentType::json())
            .insert_header(("X-Forwarded-For", "123.123.123.2"))
            .set_json(login_schema)
            .to_request();
        let resp = test::call_service(&app, req).await;

        assert!(resp.status().is_client_error());

        let body = test::read_body(resp).await;
        let body = std::str::from_utf8(&body).unwrap();
        assert_eq!(body, "Not verified");
    }
}
