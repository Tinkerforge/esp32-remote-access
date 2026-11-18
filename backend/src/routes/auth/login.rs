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
use argon2::password_hash::phc::PasswordHash;
use chrono::{Days, TimeDelta, Utc};
use db_connector::models::{refresh_tokens::RefreshToken, users::User};
use diesel::{result::Error::NotFound, ExpressionMethods, QueryDsl, SelectableHelper};
use diesel_async::{AsyncConnection, RunQueryDsl as _};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use validator::Validate;

use crate::{
    error::Error, models::token_claims::TokenClaims, rate_limit::LoginRateLimiter,
    utils::get_connection, AppState,
};

pub const MAX_TOKEN_AGE_MINUTES: i64 = 6;
const MAX_REFRESH_TOKEN_AGE_DAYS: i64 = 60;
// Allow ample room for devices and repeated logins while bounding abandoned
// sessions (for example, clients that discard their cookies after each login).
const MAX_REFRESH_TOKENS_PER_USER: i64 = 1_000;

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

    let password_hash = match PasswordHash::new(&user.login_key) {
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

    // Prune and insert atomically, serializing issuance for this account even
    // across server processes. Always reserve a slot for the new token so login
    // succeeds at the limit, including when many tokens share an expiration.
    {
        let mut conn = get_connection(state).await?;
        use db_connector::schema::refresh_tokens::dsl as refresh_tokens;

        let token = RefreshToken {
            id: token_id,
            user_id,
            expiration: exp as i64,
        };
        conn.transaction::<_, diesel::result::Error, _>(async |conn| {
            use db_connector::schema::users::dsl as users;

            users::users
                .find(user_id)
                .select(users::id)
                .for_update()
                .first::<uuid::Uuid>(conn)
                .await?;

            // All refresh tokens have the same lifetime, so expiration orders
            // them by issuance time. UUID breaks ties within the same second.
            let old_tokens = diesel::alias!(db_connector::schema::refresh_tokens as old_tokens);
            let oldest_tokens = old_tokens
                .filter(old_tokens.field(refresh_tokens::user_id).eq(user_id))
                .select(old_tokens.field(refresh_tokens::id))
                .order((
                    old_tokens.field(refresh_tokens::expiration).desc(),
                    old_tokens.field(refresh_tokens::id).desc(),
                ))
                .offset(MAX_REFRESH_TOKENS_PER_USER - 1);

            diesel::delete(
                refresh_tokens::refresh_tokens.filter(refresh_tokens::id.eq_any(oldest_tokens)),
            )
            .execute(conn)
            .await?;

            diesel::insert_into(refresh_tokens::refresh_tokens)
                .values(&token)
                .execute(conn)
                .await?;

            Ok(())
        })
        .await
        .map_err(|_| Error::InternalError)?;
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
    use std::collections::HashSet;

    use actix_web::{http::header::ContentType, test, App};

    use super::*;
    use crate::defer_async;
    use crate::{
        routes::auth::{
            register::tests::{create_user, delete_user},
            verify::tests::fast_verify,
        },
        routes::user::tests::{get_test_uuid, TestUser},
        tests::{configure, create_test_state},
    };

    async fn seed_refresh_tokens(
        state: &web::Data<AppState>,
        user_id: uuid::Uuid,
        expirations: impl IntoIterator<Item = i64>,
    ) -> Vec<RefreshToken> {
        let tokens: Vec<_> = expirations
            .into_iter()
            .map(|expiration| RefreshToken {
                id: uuid::Uuid::new_v4(),
                user_id,
                expiration,
            })
            .collect();
        let mut conn = get_connection(state).await.unwrap();
        diesel::insert_into(db_connector::schema::refresh_tokens::table)
            .values(&tokens)
            .execute(&mut conn)
            .await
            .unwrap();
        tokens
    }

    async fn refresh_token_ids(
        state: &web::Data<AppState>,
        uid: uuid::Uuid,
    ) -> HashSet<uuid::Uuid> {
        use db_connector::schema::refresh_tokens::dsl::*;
        let mut conn = get_connection(state).await.unwrap();
        refresh_tokens
            .filter(user_id.eq(uid))
            .select(id)
            .load::<uuid::Uuid>(&mut conn)
            .await
            .unwrap()
            .into_iter()
            .collect()
    }

    fn refresh_cookie_id(cookie: &str, state: &web::Data<AppState>) -> uuid::Uuid {
        let cookie = Cookie::parse(cookie.to_owned()).unwrap();
        super::super::jwt_refresh::extract_token(cookie.value().to_owned(), &state.jwt_secret)
            .unwrap()
            .0
    }

    #[actix_web::test]
    async fn refresh_token_limit_preserves_sessions_below_capacity() {
        let (user, _) = TestUser::random().await;
        let uid = get_test_uuid(&user.mail).await.unwrap();
        let state = create_test_state(None);
        let now = Utc::now().timestamp();
        let tokens = seed_refresh_tokens(
            &state,
            uid,
            (1..=MAX_REFRESH_TOKENS_PER_USER - 3).map(|i| now + i),
        )
        .await;
        let mut expected: HashSet<_> = tokens.iter().map(|token| token.id).collect();

        for _ in 0..3 {
            let cookie = create_refresh_token(&state, uid).await.unwrap();
            expected.insert(refresh_cookie_id(&cookie, &state));
        }

        assert_eq!(expected.len(), MAX_REFRESH_TOKENS_PER_USER as usize);
        assert_eq!(refresh_token_ids(&state, uid).await, expected);
    }

    #[actix_web::test]
    async fn refresh_token_limit_login_prunes_oldest_and_isolates_accounts() {
        let (user, _) = TestUser::random().await;
        let uid = get_test_uuid(&user.mail).await.unwrap();
        let (other_user, _) = TestUser::random().await;
        let other_uid = get_test_uuid(&other_user.mail).await.unwrap();
        let state = create_test_state(None);
        let now = Utc::now().timestamp();
        let other_tokens = seed_refresh_tokens(&state, other_uid, [now + 100]).await;
        // Include expired tokens and an account already above the limit.
        seed_refresh_tokens(&state, uid, [now - 2, now - 1]).await;
        let tokens = seed_refresh_tokens(
            &state,
            uid,
            (1..=MAX_REFRESH_TOKENS_PER_USER + 10).map(|i| now + i),
        )
        .await;

        let (_, refresh_token) = login_user(&user.mail, user.get_login_key().await).await;
        let new_id = super::super::jwt_refresh::extract_token(refresh_token, &state.jwt_secret)
            .unwrap()
            .0;
        let mut expected: HashSet<_> = tokens
            .iter()
            .rev()
            .take((MAX_REFRESH_TOKENS_PER_USER - 1) as usize)
            .map(|token| token.id)
            .collect();
        expected.insert(new_id);

        assert_eq!(expected.len(), MAX_REFRESH_TOKENS_PER_USER as usize);
        assert_eq!(refresh_token_ids(&state, uid).await, expected);
        assert_eq!(
            refresh_token_ids(&state, other_uid).await,
            other_tokens.iter().map(|token| token.id).collect()
        );
    }

    #[actix_web::test]
    async fn refresh_token_limit_rotation_at_capacity_preserves_other_sessions() {
        let (user, _) = TestUser::random().await;
        let uid = get_test_uuid(&user.mail).await.unwrap();
        let state = create_test_state(None);
        // Identical expiration timestamps must not cause over-eviction or
        // evict the newly issued token based on its random UUID.
        let expiration = (Utc::now() + TimeDelta::days(MAX_REFRESH_TOKEN_AGE_DAYS)).timestamp();
        seed_refresh_tokens(
            &state,
            uid,
            (0..MAX_REFRESH_TOKENS_PER_USER).map(|_| expiration),
        )
        .await;
        let cookie = create_refresh_token(&state, uid).await.unwrap();
        let mut current_id = refresh_cookie_id(&cookie, &state);
        let mut current_token = Cookie::parse(cookie).unwrap().value().to_owned();
        let mut expected = refresh_token_ids(&state, uid).await;
        assert_eq!(expected.len(), MAX_REFRESH_TOKENS_PER_USER as usize);
        assert!(expected.contains(&current_id));

        let app = test::init_service(
            App::new()
                .app_data(state.clone())
                .service(super::super::jwt_refresh::jwt_refresh),
        )
        .await;
        for _ in 0..3 {
            let req = test::TestRequest::get()
                .uri("/jwt_refresh")
                .cookie(Cookie::new("refresh_token", current_token))
                .to_request();
            let resp = test::call_service(&app, req).await;
            assert_eq!(resp.status(), 200);
            current_token = resp
                .response()
                .cookies()
                .find(|cookie| cookie.name() == "refresh_token")
                .unwrap()
                .value()
                .to_owned();
            assert!(expected.remove(&current_id));
            current_id =
                super::super::jwt_refresh::extract_token(current_token.clone(), &state.jwt_secret)
                    .unwrap()
                    .0;
            expected.insert(current_id);
            assert_eq!(refresh_token_ids(&state, uid).await, expected);
        }
    }

    #[actix_web::test]
    async fn refresh_token_limit_serializes_concurrent_issuance() {
        let (user, _) = TestUser::random().await;
        let uid = get_test_uuid(&user.mail).await.unwrap();
        // Separate pools mimic separate server processes sharing a database.
        let state = create_test_state(None);
        let other_state = create_test_state(None);
        let expiration = (Utc::now() + TimeDelta::days(1)).timestamp();
        seed_refresh_tokens(
            &state,
            uid,
            (0..MAX_REFRESH_TOKENS_PER_USER).map(|_| expiration),
        )
        .await;

        let results =
            futures_util::future::join_all((0..20).map(|i| {
                create_refresh_token(if i % 2 == 0 { &state } else { &other_state }, uid)
            }))
            .await;
        let ids = refresh_token_ids(&state, uid).await;
        assert_eq!(ids.len(), MAX_REFRESH_TOKENS_PER_USER as usize);
        for result in results {
            let cookie = result.unwrap();
            assert!(ids.contains(&refresh_cookie_id(&cookie, &state)));
        }
    }

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
