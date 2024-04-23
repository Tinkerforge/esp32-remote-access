use std::str::FromStr;

use actix_web::{cookie::Cookie, error::ErrorUnauthorized, get, web, HttpRequest, HttpResponse, Responder};
use chrono::{Duration, Utc};
use db_connector::models::{sessions::Session, users::User};
use diesel::{prelude::*, result::Error::NotFound};
use jsonwebtoken::{decode, DecodingKey, Validation};

use crate::{error::Error, middleware::get_token, models::token_claims::TokenClaims, utils::{get_connection, web_block_unpacked}, AppState};

fn extract_session(token: String, jwt_secret: &str) -> actix_web::Result<(uuid::Uuid, usize)> {
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
        Err(_err) => {
            return Err(ErrorUnauthorized("Invalid session id"))
        }
    };

    Ok((session_id, claims.exp))
}

async fn validate_token(req: &HttpRequest) -> actix_web::Result<User> {
    let token = match get_token(req, "refresh_token") {
        Some(token) => token,
        None => {
            return Err(ErrorUnauthorized("Refresh-Token is missing"))
        }
    };

    let state = req.app_data::<web::Data<AppState>>().unwrap();

    let (session_id, exp) = extract_session(token, &state.jwt_secret)?;

    let mut conn = get_connection(state)?;
    let session: Session = web_block_unpacked(move || {
        use db_connector::schema::sessions::dsl::*;

        match sessions.find(&session_id).get_result(&mut conn) {
            Ok(session) => Ok(session),
            Err(NotFound) => Err(Error::SessionDoesNotExist),
            Err(_err) => {
                Err(Error::InternalError)
            }
        }
    }).await?;

    let mut conn = get_connection(state)?;
    println!("{}, {}, {}", exp, exp as i64, Utc::now().timestamp());
    if exp < Utc::now().timestamp() as usize {
        web_block_unpacked(move || {
            use db_connector::schema::sessions::dsl::*;

            match diesel::delete(sessions.find(session_id))
                .execute(&mut conn) {
                    Ok(_) => Ok(()),
                    Err(_err) => {
                        Err(Error::InternalError)
                    }
                }
        }).await?;
        return Err(ErrorUnauthorized("Session expired"))
    } else {
        drop(conn);
    }

    let mut conn = get_connection(state)?;
    let user: User = web_block_unpacked(move || {
        use db_connector::schema::users::dsl::*;

        match users.find(session.user_id).get_result(&mut conn) {
            Ok(user) => Ok(user),
            Err(NotFound) => Err(Error::UserDoesNotExist),
            Err(_err) => {
                println!("here3");
                Err(Error::InternalError)
            }
        }
    }).await?;

    Ok(user)
}

#[get("jwt_refresh")]
pub async fn jwt_refresh(req: HttpRequest, state: web::Data<AppState>) -> actix_web::Result<impl Responder> {
    let user = validate_token(&req).await?;

    let now = Utc::now();
    let iat = now.timestamp() as usize;
    let exp = (now + Duration::minutes(super::login::MAX_TOKEN_AGE)).timestamp() as usize;
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
        .max_age(actix_web::cookie::time::Duration::minutes(super::login::MAX_TOKEN_AGE))
        .http_only(false)
        .same_site(actix_web::cookie::SameSite::None)
        .secure(true)
        .finish();

    let cookie_string = format!("{}; Partitioned;", cookie.to_string());

    Ok(HttpResponse::Ok().append_header(("Set-Cookie", cookie_string)).body(""))
}

#[cfg(test)]
mod tests {
    use actix_web::{cookie::Cookie, test::{self, TestRequest}, App};
    use chrono::{Duration, Utc};
    use jsonwebtoken::{decode, encode, Validation};
    use rand::{distributions::Alphanumeric, Rng};

    use crate::{models::token_claims::TokenClaims, routes::user::tests::TestUser, tests::configure};

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
        }
        assert_eq!(bitmap, 1);
    }

    #[actix_web::test]
    async fn no_refresh_token() {
        let app = App::new().configure(configure).service(jwt_refresh);
        let app = test::init_service(app).await;

        let (mut user, _) = TestUser::random().await;
        user.login().await;

        let req = TestRequest::get()
            .uri("/jwt_refresh")
            .to_request();

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
        let claims = decode::<TokenClaims>(token, &jsonwebtoken::DecodingKey::from_secret(std::env::var("JWT_SECRET").unwrap().as_bytes()), &Validation::default()).unwrap();

        let now = Utc::now();
        let iat = (now.timestamp() as usize) - 120;
        let exp = ((now + Duration::minutes(1)).timestamp() as usize) - 120;
        let claims = TokenClaims {
            iat,
            exp,
            sub: claims.claims.sub
        };

        let token = encode(&jsonwebtoken::Header::default(), &claims, &jsonwebtoken::EncodingKey::from_secret(std::env::var("JWT_SECRET").unwrap().as_bytes())).unwrap();

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
