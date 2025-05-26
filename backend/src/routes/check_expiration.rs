use actix_web::{post, web, Responder};
use db_connector::models::{recovery_tokens::RecoveryToken, verification::Verification};
use serde::{Deserialize, Serialize};
use diesel::{prelude::*, result::Error::NotFound};
use utoipa::ToSchema;

use crate::{error::Error, rate_limit::IPRateLimiter, utils::{get_connection, parse_uuid}, AppState};

#[derive(Deserialize, Serialize, ToSchema)]
pub enum TokenType {
    Recovery,
    Verification,
}

#[derive(Deserialize, Serialize, ToSchema)]
pub struct CheckExpirationRequest {
    pub token_type: TokenType,
    pub token: String,
}

async fn check_recovery_token(
    state: &web::Data<AppState>,
    token: uuid::Uuid,
) -> actix_web::Result<bool> {
    let mut conn = get_connection(state)?;
    let valid = web::block(move || {
        use db_connector::schema::recovery_tokens::dsl::*;

        let valid = match recovery_tokens
            .filter(id.eq(token))
            .get_result::<RecoveryToken>(&mut conn)
        {
            Ok(_) => true,
            Err(NotFound) => false,
            Err(_) => return Err(Error::InternalError),
        };

        Ok(valid)
    }).await??;

    Ok(valid)
}

async fn check_verification_token(
    state: &web::Data<AppState>,
    token: uuid::Uuid,
) -> actix_web::Result<bool> {
    let mut conn = get_connection(state)?;
    let valid = web::block(move || {
        use db_connector::schema::verification::dsl::*;

        let valid = match verification
            .filter(id.eq(token))
            .get_result::<Verification>(&mut conn)
        {
            Ok(_) => true,
            Err(NotFound) => false,
            Err(_err) => {
                println!("Error checking verification token: {:?}", _err);
                return Err(Error::InternalError)
            },
        };

        Ok(valid)
    }).await??;

    Ok(valid)
}

#[utoipa::path(
    request_body = CheckExpirationRequest,
    responses(
        (status = 200, description = "Check was successful", body = bool),
        (status = 400, description = "Invalid request data"),
        (status = 429, description = "Rate limit exceeded"),
        (status = 500, description = "Internal server error"),
    )
)]
#[post("/check_expiration")]
pub async fn check_expiration(
    state: web::Data<AppState>,
    data: web::Json<CheckExpirationRequest>,
    rate_limiter: web::Data<IPRateLimiter>,
    req: actix_web::HttpRequest,
) -> actix_web::Result<impl Responder> {
    rate_limiter.check(&req)?;

    let token = parse_uuid(&data.token)?;

    let valid = match data.token_type {
        TokenType::Recovery => {
            check_recovery_token(&state, token).await?
        },
        TokenType::Verification => {
            check_verification_token(&state, token).await?
        },
    };

    Ok(actix_web::HttpResponse::Ok().json(valid))
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::{test, App};
    use db_connector::test_connection_pool;
    use db_connector::models::{recovery_tokens::RecoveryToken, verification::Verification};
    use uuid::Uuid;
    use chrono::Utc;
    use crate::routes::user::tests::{TestUser, get_test_uuid};
    use crate::tests::configure;

    #[actix_web::test]
    async fn test_valid_recovery_token() {
        let (mut user, mail) = TestUser::random().await;
        user.login().await;
        let user_id = get_test_uuid(&mail).unwrap();
        let token_id = Uuid::new_v4();
        let pool = test_connection_pool();
        let mut conn = pool.get().unwrap();
        let token = RecoveryToken {
            id: token_id,
            user_id,
            created: Utc::now().timestamp(),
        };
        diesel::insert_into(db_connector::schema::recovery_tokens::dsl::recovery_tokens)
            .values(&token)
            .execute(&mut conn)
            .unwrap();

        let app = App::new()
            .configure(configure)
            .service(check_expiration);
        let app = test::init_service(app).await;
        let req = test::TestRequest::post()
            .uri("/check_expiration")
            .append_header(("X-Forwarded-For", "123.123.123.3"))
            .set_json(CheckExpirationRequest {
                token_type: TokenType::Recovery,
                token: token_id.to_string(),
            })
            .to_request();
        let resp = test::call_service(&app, req).await;
        println!("Response: {:?}", resp);
        assert!(resp.status().is_success());
        let valid: bool = test::read_body_json(resp).await;
        assert!(valid);

        diesel::delete(
            db_connector::schema::recovery_tokens::dsl::recovery_tokens
                .filter(db_connector::schema::recovery_tokens::dsl::id.eq(token_id))
        ).execute(&mut conn)
            .unwrap();
    }

    #[actix_web::test]
    async fn test_invalid_recovery_token() {
        let (mut user, _) = TestUser::random().await;
        user.login().await;

        let app = App::new()
            .configure(configure)
            .service(check_expiration);
        let app = test::init_service(app).await;

        let req = test::TestRequest::post()
            .uri("/check_expiration")
            .append_header(("X-Forwarded-For", "123.123.123.3"))
            .set_json(&CheckExpirationRequest {
                token_type: TokenType::Recovery,
                token: Uuid::new_v4().to_string(),
            })
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_success());
        let valid: bool = test::read_body_json(resp).await;
        assert!(!valid);
    }

    #[actix_web::test]
    async fn test_valid_verification_token() {
        let (mut user, mail) = TestUser::random().await;
        user.login().await;
        let user_id = get_test_uuid(&mail).unwrap();
        let token_id = Uuid::new_v4();
        let pool = test_connection_pool();
        let mut conn = pool.get().unwrap();
        let token = Verification {
            id: token_id,
            user: user_id,
            expiration: Utc::now().naive_utc() + chrono::Duration::days(1),
        };
        diesel::insert_into(db_connector::schema::verification::dsl::verification)
            .values(&token)
            .execute(&mut conn)
            .unwrap();

        let app = App::new()
            .configure(configure)
            .service(check_expiration);
        let app = test::init_service(app).await;

        let req = test::TestRequest::post()
            .uri("/check_expiration")
            .append_header(("X-Forwarded-For", "123.123.123.3"))
            .set_json(&CheckExpirationRequest {
                token_type: TokenType::Verification,
                token: token_id.to_string(),
            })
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_success());
        let valid: bool = test::read_body_json(resp).await;
        assert!(valid);
    }

    #[actix_web::test]
    async fn test_invalid_verification_token() {
        let (mut user, _) = TestUser::random().await;
        user.login().await;

        let app = App::new()
            .configure(configure)
            .service(check_expiration);
        let app = test::init_service(app).await;

        let req = test::TestRequest::post()
            .uri("/check_expiration")
            .append_header(("X-Forwarded-For", "123.123.123.3"))
            .set_json(&CheckExpirationRequest {
                token_type: TokenType::Verification,
                token: Uuid::new_v4().to_string(),
            })
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_success());
        let valid: bool = test::read_body_json(resp).await;
        assert!(!valid);
    }
}
