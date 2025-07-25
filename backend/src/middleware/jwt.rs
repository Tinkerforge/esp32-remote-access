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

use actix_web::{
    dev::{forward_ready, Payload, Service, ServiceRequest, ServiceResponse, Transform},
    error::{ErrorInternalServerError, ErrorUnauthorized},
    web, Error, FromRequest, HttpMessage, HttpRequest,
};
use chrono::Utc;
use futures_util::future::LocalBoxFuture;
use jsonwebtoken::{decode, DecodingKey, Validation};
use std::future::{ready, Ready};

use crate::{models::token_claims::TokenClaims, AppState};

use super::get_token;

pub struct JwtMiddleware;

impl<S, B> Transform<S, ServiceRequest> for JwtMiddleware
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error>,
    S::Future: 'static,
    B: 'static,
{
    type Error = Error;
    type Response = ServiceResponse<B>;
    type InitError = ();
    type Future = Ready<Result<Self::Transform, Self::InitError>>;
    type Transform = JwtService<S>;

    fn new_transform(&self, service: S) -> Self::Future {
        ready(Ok(JwtService { service }))
    }
}

// Trait to use JwtMiddleware as an extractor
impl FromRequest for JwtMiddleware {
    type Error = actix_web::Error;
    type Future = Ready<Result<Self, Self::Error>>;

    fn from_request(req: &HttpRequest, _: &mut Payload) -> Self::Future {
        if let Err(err) = validate_token(req) {
            return ready(Err(err));
        }

        ready(Ok(JwtMiddleware {}))
    }
}

pub struct JwtService<S> {
    service: S,
}

impl<S, B> Service<ServiceRequest> for JwtService<S>
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error>,
    S::Future: 'static,
    B: 'static,
{
    type Error = Error;
    type Future = LocalBoxFuture<'static, Result<Self::Response, Self::Error>>;
    type Response = ServiceResponse<B>;

    forward_ready!(service);

    fn call(&self, req: ServiceRequest) -> Self::Future {
        if let Err(err) = validate_token(req.request()) {
            return Box::pin(ready(Err(err)));
        }

        let fut = self.service.call(req);
        Box::pin(fut)
    }
}

fn validate_token(req: &HttpRequest) -> Result<(), Error> {
    let token = match get_token(req, "access_token") {
        Some(token) => token,
        None => return Err(ErrorUnauthorized("Jwt-Token is missing")),
    };

    let data = req.app_data::<web::Data<AppState>>().unwrap();
    let claims = match decode::<TokenClaims>(
        &token,
        &DecodingKey::from_secret(data.jwt_secret.as_bytes()),
        &Validation::default(),
    ) {
        Ok(claims) => claims.claims,
        Err(_err) => {
            return Err(ErrorUnauthorized("Invalid jwt token"));
        }
    };

    let now = Utc::now();
    if now.timestamp() as usize > claims.exp {
        return Err(ErrorUnauthorized("Jwt token expired"));
    }

    let user_id = match uuid::Uuid::parse_str(claims.sub.as_str()) {
        Ok(id) => id,
        Err(_err) => return Err(ErrorInternalServerError("")),
    };

    req.extensions_mut().insert::<uuid::Uuid>(user_id);

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{routes::user::tests::TestUser, tests::configure};
    use actix_web::{cookie::Cookie, get, test, App, Responder};
    use chrono::{Duration, Utc};
    use rand::{distr::Alphanumeric, Rng};

    #[get("/hello")]
    async fn with_extractor(_: JwtMiddleware) -> impl Responder {
        "Hello!"
    }

    #[get("/hello")]
    async fn without_extractor() -> impl Responder {
        "Hello!"
    }

    // Since the validation logic is the same testing one good and one bad test should be enought for the wrapper

    #[actix_web::test]
    async fn test_valid_token_extractor() {
        let (mut user, _) = TestUser::random().await;
        let token = user.login().await;

        let app = App::new().configure(configure).service(with_extractor);
        let app = test::init_service(app).await;

        let cookie = Cookie::new("access_token", token);
        let req = test::TestRequest::get()
            .uri("/hello")
            .cookie(cookie)
            .to_request();

        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_success());
    }

    #[actix_web::test]
    async fn test_valid_token_middleware() {
        let (mut user, _) = TestUser::random().await;
        let token = user.login().await;

        let app = App::new()
            .configure(configure)
            .service(without_extractor)
            .wrap(JwtMiddleware);
        let app = test::init_service(app).await;

        let cookie = Cookie::new("access_token", token);
        let req = test::TestRequest::get()
            .uri("/hello")
            .cookie(cookie)
            .to_request();

        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_success());
    }

    #[actix_web::test]
    async fn test_no_token_extractor() {
        let (mut user, _) = TestUser::random().await;
        user.login().await;

        let app = App::new().configure(configure).service(with_extractor);
        let app = test::init_service(app).await;

        let req = test::TestRequest::get().uri("/hello").to_request();

        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_client_error());
    }

    #[actix_web::test]
    async fn garbage_token() {
        let (mut user, _) = TestUser::random().await;
        user.login().await;

        let app = App::new().configure(configure).service(with_extractor);
        let app = test::init_service(app).await;

        let token: String = rand::rng()
            .sample_iter(&Alphanumeric)
            .take(1024)
            .map(char::from)
            .collect();

        let req = test::TestRequest::get()
            .uri("/hello")
            .cookie(Cookie::new("access_token", token))
            .to_request();

        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_client_error());
    }

    #[actix_web::test]
    async fn fake_token() {
        let (mut user, _) = TestUser::random().await;
        user.login().await;

        let app = App::new().configure(configure).service(with_extractor);
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

        let jwt_secret: String = rand::rng()
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
            .uri("/hello")
            .cookie(Cookie::new("access_token", token))
            .to_request();

        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 401);
    }

    #[actix_web::test]
    async fn expired_token() {
        let (mut user, username) = TestUser::random().await;
        user.login().await;

        let app = App::new()
            .configure(configure)
            .service(without_extractor)
            .wrap(JwtMiddleware);
        let app = test::init_service(app).await;

        let now = Utc::now();
        let iat = now.timestamp() as usize - 300;
        let exp = (now + Duration::minutes(1)).timestamp() as usize - 120;
        let claims = TokenClaims {
            iat,
            exp,
            sub: username,
        };

        let token = jsonwebtoken::encode(
            &jsonwebtoken::Header::default(),
            &claims,
            &jsonwebtoken::EncodingKey::from_secret(
                std::env::var("JWT_SECRET")
                    .expect("JWT_SECRET must be set")
                    .as_bytes(),
            ),
        )
        .unwrap();

        let req = test::TestRequest::get()
            .uri("/hello")
            .cookie(Cookie::new("access_token", token))
            .to_request();

        let resp = crate::tests::call_service(&app, req).await;

        println!("{}", resp.status());
        println!("{:?}", resp.response().body());
        assert_eq!(resp.status(), 401);
    }
}
