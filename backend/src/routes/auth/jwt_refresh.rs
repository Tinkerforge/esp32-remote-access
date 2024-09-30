use std::str::FromStr;

use actix_web::{
    cookie::Cookie, error::ErrorUnauthorized, get, web, HttpRequest, HttpResponse, Responder,
};
use chrono::{Duration, Utc};
use db_connector::models::{refresh_tokens::RefreshToken, users::User};
use diesel::{prelude::*, result::Error::NotFound};
use jsonwebtoken::{decode, DecodingKey, Validation};

use crate::{
    error::Error,
    middleware::get_token,
    models::token_claims::TokenClaims,
    routes::auth::login::create_refresh_token,
    utils::{get_connection, web_block_unpacked},
    AppState,
};

pub fn extract_token(token: String, jwt_secret: &str) -> actix_web::Result<(uuid::Uuid, usize)> {
    let claims = match decode::<TokenClaims>(
        &token,
        &DecodingKey::from_secret(jwt_secret.as_bytes()),
        &Validation::default(),
    ) {
        Ok(claims) => claims.claims,
        Err(_err) => {
            return Err(ErrorUnauthorized("Invalid jwt token"));
        }
    };

    let session_id = match uuid::Uuid::from_str(&claims.sub) {
        Ok(id) => id,
        Err(_err) => return Err(ErrorUnauthorized("Invalid session id")),
    };

    Ok((session_id, claims.exp))
}

async fn validate_token(req: &HttpRequest) -> actix_web::Result<User> {
    let token = match get_token(req, "refresh_token") {
        Some(token) => token,
        None => return Err(ErrorUnauthorized("Refresh-Token is missing")),
    };

    let state = req.app_data::<web::Data<AppState>>().unwrap();

    let (token_id, exp) = extract_token(token, &state.jwt_secret)?;

    let mut conn = get_connection(state)?;
    let refresh_token: RefreshToken = web_block_unpacked(move || {
        use db_connector::schema::refresh_tokens::dsl::*;

        match refresh_tokens.find(&token_id).get_result(&mut conn) {
            Ok(session) => Ok(session),
            Err(NotFound) => Err(Error::SessionDoesNotExist),
            Err(_err) => Err(Error::InternalError),
        }
    })
    .await?;

    delete_refresh_token(token_id, state).await?;
    if exp < Utc::now().timestamp() as usize {
        return Err(ErrorUnauthorized("Session expired"));
    }

    let mut conn = get_connection(state)?;
    let user: User = web_block_unpacked(move || {
        use db_connector::schema::users::dsl::*;

        match users.find(refresh_token.user_id).get_result(&mut conn) {
            Ok(user) => Ok(user),
            Err(NotFound) => Err(Error::UserDoesNotExist),
            Err(_err) => {
                println!("here3");
                Err(Error::InternalError)
            }
        }
    })
    .await?;

    Ok(user)
}

pub async fn delete_refresh_token(
    token_id: uuid::Uuid,
    state: &web::Data<AppState>,
) -> actix_web::Result<()> {
    let mut conn = get_connection(state)?;

    web_block_unpacked(move || {
        use db_connector::schema::refresh_tokens::dsl::*;

        match diesel::delete(refresh_tokens.find(token_id)).execute(&mut conn) {
            Ok(_) => Ok(()),
            Err(_err) => Err(Error::InternalError),
        }
    })
    .await?;
    Ok(())
}

/// Refresh the jwt-token. A valid refresh-token is needed.
#[utoipa::path(
    context_path = "/auth",
    responses(
        (status = 200),
        (status = 401, description = "The refresh token was invalid", body = String)
    ),
    security(
        ("refresh" = [])
    )
)]
#[get("/jwt_refresh")]
pub async fn jwt_refresh(
    req: HttpRequest,
    state: web::Data<AppState>,
) -> actix_web::Result<impl Responder> {
    let user = match validate_token(&req).await {
        Ok(u) => u,
        Err(err) => {
            let access_token = Cookie::build("access_token", "")
                .path("/")
                .max_age(actix_web::cookie::time::Duration::new(-1, 0))
                .http_only(true)
                .same_site(actix_web::cookie::SameSite::Strict)
                .finish();
            let refresh_token = Cookie::build("refresh_token", "")
                .path("/")
                .max_age(actix_web::cookie::time::Duration::new(-1, 0))
                .same_site(actix_web::cookie::SameSite::Strict)
                .http_only(true)
                .finish();

            return Ok(HttpResponse::Unauthorized()
                .cookie(access_token)
                .cookie(refresh_token)
                .body(err.to_string()));
        }
    };

    let now = Utc::now();
    let iat = now.timestamp() as usize;
    let exp = (now + Duration::minutes(super::login::MAX_TOKEN_AGE_MINUTES)).timestamp() as usize;
    let claims = TokenClaims {
        iat,
        exp,
        sub: user.id.to_string(),
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
            super::login::MAX_TOKEN_AGE_MINUTES,
        ))
        .http_only(true)
        .same_site(actix_web::cookie::SameSite::Strict)
        .secure(true)
        .finish();

    let cookie_string = format!("{}; Partitioned;", cookie.to_string());

    let refresh_cookie = create_refresh_token(&state, user.id).await?;

    Ok(HttpResponse::Ok()
        .append_header(("Set-Cookie", cookie_string))
        .append_header(("Set-Cookie", refresh_cookie))
        .body(""))
}

#[cfg(test)]
mod tests {
    use actix_web::{
        cookie::Cookie,
        test::{self, TestRequest},
        App,
    };
    use chrono::{Duration, Utc};
    use jsonwebtoken::{decode, encode, Validation};
    use rand::{distributions::Alphanumeric, Rng};

    use crate::{
        models::token_claims::TokenClaims, routes::user::tests::TestUser, tests::configure,
    };

    use super::jwt_refresh;

    #[actix_web::test]
    async fn request_new_token() {
        let app = App::new().configure(configure).service(jwt_refresh);
        let app = test::init_service(app).await;

        let (mut user, _) = TestUser::random().await;
        user.login().await;
        let token = user.get_refresh_token();

        let req = TestRequest::get()
            .uri("/jwt_refresh")
            .cookie(Cookie::new("refresh_token", token))
            .to_request();

        let resp = test::call_service(&app, req).await;
        println!("{:?}", resp.response().body());
        assert_eq!(resp.status(), 200);

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
    async fn no_refresh_token() {
        let app = App::new().configure(configure).service(jwt_refresh);
        let app = test::init_service(app).await;

        let (mut user, _) = TestUser::random().await;
        user.login().await;

        let req = TestRequest::get().uri("/jwt_refresh").to_request();

        let resp = test::call_service(&app, req).await;
        println!("{:?}", resp.response().body());
        assert_eq!(resp.status(), 401);
    }

    #[actix_web::test]
    async fn garbage_token() {
        let (mut user, _) = TestUser::random().await;
        user.login().await;

        let app = App::new().configure(configure).service(jwt_refresh);
        let app = test::init_service(app).await;

        let token: String = rand::thread_rng()
            .sample_iter(&Alphanumeric)
            .take(1024)
            .map(char::from)
            .collect();

        let req = test::TestRequest::get()
            .uri("/jwt_refresh")
            .cookie(Cookie::new("refresh_token", token))
            .to_request();

        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 401);
    }

    #[actix_web::test]
    async fn fake_token() {
        let (mut user, _) = TestUser::random().await;
        user.login().await;

        let app = App::new().configure(configure).service(jwt_refresh);
        let app = test::init_service(app).await;

        let id = uuid::Uuid::new_v4();
        let now = Utc::now();
        let iat = now.timestamp() as usize;
        let exp = (now + Duration::minutes(60)).timestamp() as usize;
        let claims = TokenClaims {
            iat,
            exp,
            sub: id.to_string(),
        };

        let jwt_secret: String = rand::thread_rng()
            .sample_iter(&Alphanumeric)
            .take(1024)
            .map(char::from)
            .collect();

        let token = jsonwebtoken::encode(
            &jsonwebtoken::Header::default(),
            &claims,
            &jsonwebtoken::EncodingKey::from_secret(jwt_secret.as_ref()),
        )
        .unwrap();

        let req = test::TestRequest::get()
            .uri("/jwt_refresh")
            .cookie(Cookie::new("refresh_token", token))
            .to_request();

        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 401);
    }

    #[actix_web::test]
    async fn expired_session() {
        let (mut user, _) = TestUser::random().await;
        user.login().await;
        let token = user.get_refresh_token();
        let claims = decode::<TokenClaims>(
            token,
            &jsonwebtoken::DecodingKey::from_secret(
                std::env::var("JWT_SECRET").unwrap().as_bytes(),
            ),
            &Validation::default(),
        )
        .unwrap();

        let now = Utc::now();
        let iat = (now.timestamp() as usize) - 120;
        let exp = ((now + Duration::minutes(1)).timestamp() as usize) - 120;
        let claims = TokenClaims {
            iat,
            exp,
            sub: claims.claims.sub,
        };

        let token = encode(
            &jsonwebtoken::Header::default(),
            &claims,
            &jsonwebtoken::EncodingKey::from_secret(
                std::env::var("JWT_SECRET").unwrap().as_bytes(),
            ),
        )
        .unwrap();

        let app = App::new().configure(configure).service(jwt_refresh);
        let app = test::init_service(app).await;

        let req = TestRequest::get()
            .uri("/jwt_refresh")
            .cookie(Cookie::new("refresh_token", token))
            .to_request();

        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 401);
    }
}
