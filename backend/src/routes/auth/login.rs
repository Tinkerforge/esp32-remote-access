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
use db_connector::models::users::User;
use diesel::{
    prelude::*,
    r2d2::{ConnectionManager, PooledConnection},
    result::Error::NotFound,
};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use validator::Validate;

use crate::{error::Error, models::token_claims::TokenClaims, AppState};

#[derive(Serialize, Deserialize, Clone, Debug, Validate, ToSchema)]
pub struct LoginSchema {
    #[validate(email)]
    pub email: String,
    pub password: String,
}

pub enum FindBy {
    Uuid(uuid::Uuid),
    Email(String),
}

pub async fn validate_password(
    pass: &str,
    identifier: FindBy,
    mut conn: PooledConnection<ConnectionManager<PgConnection>>,
) -> Result<uuid::Uuid, actix_web::Error> {
    use db_connector::schema::users::dsl::*;

    let result = web::block(move || match identifier {
        FindBy::Email(mail) => users
            .filter(email.eq(mail))
            .select(User::as_select())
            .get_result(&mut conn),
        FindBy::Uuid(uid) => users
            .find(uid)
            .select(User::as_select())
            .get_result(&mut conn),
    })
    .await
    .unwrap();

    let user: User = match result {
        Ok(data) => data,
        Err(NotFound) => return Err(Error::WrongCredentials.into()),
        Err(_err) => return Err(Error::InternalError.into()),
    };

    if !user.email_verified {
        return Err(Error::NotVerified.into());
    }

    let password_hash = match PasswordHash::new(&user.password) {
        Ok(hash) => hash,
        Err(_err) => return Err(Error::InternalError.into()),
    };

    match Argon2::default().verify_password(pass.as_bytes(), &password_hash) {
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

    let mail = data.email.to_lowercase();
    let uuid = validate_password(&data.password, FindBy::Email(mail), conn).await?;

    let max_token_age = 60;

    let now = Utc::now();
    let iat = now.timestamp() as usize;
    let exp = (now + Duration::minutes(max_token_age)).timestamp() as usize;
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
        .max_age(actix_web::cookie::time::Duration::minutes(max_token_age))
        .http_only(false)
        .same_site(actix_web::cookie::SameSite::None)
        .secure(true)
        .finish();

    Ok(HttpResponse::Ok().cookie(cookie).body(""))
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

    pub async fn login_user(mail: &str, password: Option<String>) -> String {
        let app = App::new().configure(configure).service(login);
        let app = test::init_service(app).await;

        let password = if let Some(pass) = password {
            pass
        } else {
            "TestTestTest".to_string()
        };

        let login_schema = LoginSchema {
            email: mail.to_string(),
            password,
        };
        let req = test::TestRequest::post()
            .uri("/login")
            .insert_header(ContentType::json())
            .set_json(login_schema)
            .to_request();
        let resp = test::call_service(&app, req).await;

        assert!(resp.status().is_success());

        let cookies = resp.response().cookies();
        for cookie in cookies {
            if cookie.name() == "access_token" {
                return cookie.value().to_string();
            }
        }
        assert!(false);

        String::new()
    }

    pub async fn verify_and_login_user(mail: &str) -> String {
        fast_verify(mail);

        login_user(mail, None).await
    }

    #[actix_web::test]
    async fn test_valid_login() {
        let mail = "login@test.invalid";
        create_user(mail).await;
        defer!(delete_user(mail));

        let app = App::new().configure(configure).service(login);
        let app = test::init_service(app).await;
        let login_schema = LoginSchema {
            email: mail.to_string(),
            password: "TestTestTest".to_string(),
        };
        fast_verify(mail);

        let req = test::TestRequest::post()
            .uri("/login")
            .insert_header(ContentType::json())
            .set_json(login_schema)
            .to_request();
        let resp = test::call_service(&app, req).await;

        assert!(resp.status().is_success());

        let cookies = resp.response().cookies();
        let mut valid = false;
        for cookie in cookies {
            if cookie.name() == "access_token" {
                valid = true;
                break;
            }
        }
        assert!(valid);
    }

    #[actix_web::test]
    async fn test_invalid_email() {
        let mail = "invalid_mail@test.invalid";
        create_user(mail).await;
        defer!(delete_user(mail));

        let app = App::new().configure(configure).service(login);
        let app = test::init_service(app).await;
        let login_schema = LoginSchema {
            email: "invalid@test.invalid".to_string(),
            password: "TestTestTest".to_string(),
        };
        fast_verify(mail);

        let req = test::TestRequest::post()
            .uri("/login")
            .insert_header(ContentType::json())
            .set_json(login_schema)
            .to_request();
        let resp = test::call_service(&app, req).await;

        println!("{}", resp.status());
        assert!(resp.status().is_client_error());

        let body = test::read_body(resp).await;
        let body = std::str::from_utf8(&body).unwrap();
        assert_eq!(body, "Wrong username or password");
    }

    #[actix_web::test]
    async fn test_invalid_password() {
        let mail = "invalid_pass@test.invalid";
        create_user(mail).await;
        defer!(delete_user(mail));

        let app = App::new().configure(configure).service(login);
        let app = test::init_service(app).await;
        let login_schema = LoginSchema {
            email: "invalid_pass@test.invalid".to_string(),
            password: "TestTestTest1".to_string(),
        };
        fast_verify(mail);

        let req = test::TestRequest::post()
            .uri("/login")
            .insert_header(ContentType::json())
            .set_json(login_schema)
            .to_request();

        let resp = test::call_service(&app, req).await;
        println!("{}", resp.status());

        assert!(resp.status().is_client_error());

        let body = test::read_body(resp).await;
        let body = std::str::from_utf8(&body).unwrap();
        assert_eq!(body, "Wrong username or password");
    }

    #[actix_web::test]
    async fn test_unverified() {
        let mail = "unverified_login@test.invalid";
        create_user(mail).await;
        defer!(delete_user(mail));

        let app = App::new().configure(configure).service(login);
        let app = test::init_service(app).await;
        let login_schema = LoginSchema {
            email: mail.to_string(),
            password: "TestTestTest".to_string(),
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
