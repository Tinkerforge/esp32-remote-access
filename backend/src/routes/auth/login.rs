use actix_web::{cookie::Cookie, post, web, HttpResponse, Responder};
use actix_web_validator::Json;
use argon2::{Argon2, PasswordHash, PasswordVerifier};
use chrono::{Duration, Utc};
use db_connector::model::users::User;
use diesel::prelude::*;

use crate::{model::{login::LoginSchema, token_claims::TokenClaims}, AppState};


#[post("/login")]
pub async fn login(state: web::Data<AppState>, data: Json<LoginSchema>) -> impl Responder {
    use db_connector::schema::users::dsl::*;

    let mut conn = match state.pool.get() {
        Ok(conn) => conn,
        Err(_err) => {
            return HttpResponse::InternalServerError().body("")
        }
    };

    let result = users.filter(email.eq(&data.email.to_lowercase()))
        .select(User::as_select())
        .load(&mut conn);
    let user: User = match result {
        Ok(data) => {
            if data.len() == 1 {
                data[0].clone()
            } else {
                return HttpResponse::BadRequest().body("")
            }
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

    match Argon2::default().verify_password(data.password.as_bytes(), &password_hash) {
        Ok(_) => (),
        Err(_err) => {
            return HttpResponse::BadRequest().body("")
        }
    }

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

    let cookie = Cookie::build("token", token)
        .path("/")
        .max_age(actix_web::cookie::time::Duration::minutes(max_token_age))
        .http_only(false)
        .finish();

    HttpResponse::Ok().cookie(cookie).body("Logged in")
}

#[cfg(test)]
mod tests {
    use actix_web::{http::header::ContentType, test, App};

    use super::*;
    use crate::{routes::auth::register::tests::{create_user, delete_test_user}, tests::configure};
    use crate::defer;

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
        println!("{}", resp.status());
        assert!(resp.status().is_success());
        let cookies = resp.response().cookies();
        let mut valid = false;
        for cookie in cookies {
            if cookie.name() == "token" {
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
