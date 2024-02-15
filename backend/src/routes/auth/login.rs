use std::time::Instant;

use actix_web::{cookie::Cookie, post, web, HttpResponse, Responder};
use actix_web_validator::Json;
use argon2::{Argon2, PasswordHash, PasswordVerifier};
use chrono::{Duration, Utc};
use db_connector::models::users::User;
use diesel::{prelude::*, result::Error::NotFound};

use crate::{models::{login::LoginSchema, token_claims::TokenClaims}, AppState};


#[post("/login")]
pub async fn login(state: web::Data<AppState>, data: Json<LoginSchema>) -> impl Responder {
    use db_connector::schema::users::dsl::*;

    let now = Instant::now();

    let mut conn = match state.pool.get() {
        Ok(conn) => conn,
        Err(_err) => {
            return HttpResponse::InternalServerError().body("")
        }
    };

    let mail = data.email.to_lowercase();
    let result = web::block(move|| {
        users.filter(email.eq(mail))
            .select(User::as_select())
            .get_result(&mut conn)
    }).await.unwrap();

    println!("Took {}ms to get user from database", now.elapsed().as_millis());

    let user: User = match result {
        Ok(data) => data,
        Err(NotFound) => {
            return HttpResponse::BadRequest().body("")
        },
        Err(_err) => {
            return HttpResponse::InternalServerError().body("")
        }
    };

    let password_hash = match PasswordHash::new(&user.password) {
        Ok(hash) => hash,
        Err(_err) => {
            return HttpResponse::InternalServerError().body("")
        }
    };

    println!("Took {}ms to hash password", now.elapsed().as_millis());

    match Argon2::default().verify_password(data.password.as_bytes(), &password_hash) {
        Ok(_) => (),
        Err(_err) => {
            println!("Took {}ms to verify password", now.elapsed().as_millis());
            return HttpResponse::BadRequest().body("")
        }
    }

    println!("Took {}ms to verify password", now.elapsed().as_millis());

    let max_token_age = 60;

    let now = Utc::now();
    let iat = now.timestamp() as usize;
    let exp = (now + Duration::minutes(max_token_age)).timestamp() as usize;
    let claims = TokenClaims {
        iat,
        exp,
        sub: user.id.to_string()
    };

    let token = match jsonwebtoken::encode(
        &jsonwebtoken::Header::default(),
        &claims,
        &jsonwebtoken::EncodingKey::from_secret(state.jwt_secret.as_ref())
    ) {
        Ok(token) => token,
        Err(_err) => {
            return HttpResponse::InternalServerError().body("")
        }
    };

    let cookie = Cookie::build("access_token", token)
        .path("/")
        .max_age(actix_web::cookie::time::Duration::minutes(max_token_age))
        .http_only(false)
        .finish();

    HttpResponse::Ok().cookie(cookie).body("Logged in")
}

#[cfg(test)]
pub(crate) mod tests {
    use actix_web::{http::header::ContentType, test, App};

    use super::*;
    use crate::{routes::auth::register::tests::{create_user, delete_test_user}, tests::configure};
    use crate::defer;

    pub async fn login_user(mail: &str) -> String {
        let app = App::new().configure(configure ).service(login);
        let app = test::init_service(app).await;
        let login_schema = LoginSchema {
            email: mail.to_string(),
            password: "TestTestTest".to_string()
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
        };
        assert!(false);

        String::new()
    }

    #[actix_web::test]
    async fn test_valid_login() {
        let mail = "login@test.de";
        create_user(mail).await;
        defer!(delete_test_user(mail));

        let app = App::new().configure(configure ).service(login);
        let app = test::init_service(app).await;
        let login_schema = LoginSchema {
            email: mail.to_string(),
            password: "TestTestTest".to_string()
        };
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
        let mail = "invalid_mail@test.de";
        create_user(mail).await;
        defer!(delete_test_user(mail));

        let app = App::new().configure(configure ).service(login);
        let app = test::init_service(app).await;
        let login_schema = LoginSchema {
            email: "invalid@test.de".to_string(),
            password: "TestTestTest".to_string()
        };
        let req = test::TestRequest::post()
            .uri("/login")
            .insert_header(ContentType::json())
            .set_json(login_schema)
            .to_request();
        let resp = test::call_service(&app, req).await;
        println!("{}", resp.status());
        assert!(resp.status().is_client_error());
    }

    #[actix_web::test]
    async fn test_invalid_password() {
        let mail = "invalid_pass@test.de";
        create_user(mail).await;
        defer!(delete_test_user(mail));

        let app = App::new().configure(configure ).service(login);
        let app = test::init_service(app).await;
        let login_schema = LoginSchema {
            email: "invalid_pass@test.de".to_string(),
            password: "TestTestTest1".to_string()
        };
        let req = test::TestRequest::post()
            .uri("/login")
            .insert_header(ContentType::json())
            .set_json(login_schema)
            .to_request();
        let resp = test::call_service(&app, req).await;
        println!("{}", resp.status());
        assert!(resp.status().is_client_error());
    }
}
