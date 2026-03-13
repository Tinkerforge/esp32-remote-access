use actix_web::{get, web, HttpRequest, HttpResponse, Responder};
use askama::Template;
use db_connector::models::recovery_tokens::RecoveryToken;
use diesel::prelude::*;
use serde::Deserialize;
use utoipa::IntoParams;
use uuid::Uuid;

use crate::{
    branding,
    error::Error,
    rate_limit::LoginRateLimiter,
    routes::user::{get_user, get_user_id},
    utils::{self, get_connection, web_block_unpacked},
    AppState,
};

#[derive(Deserialize, IntoParams)]
struct StartRecoveryQuery {
    email: String,
}

#[derive(Template)]
#[template(path = "start_recovery_de.html")]
struct StartRecoveryDETemplate<'a> {
    name: &'a str,
    link: &'a str,
    brand: branding::Brand,
}

#[derive(Template)]
#[template(path = "start_recovery_en.html")]
struct StartRecoveryENTemplate<'a> {
    name: &'a str,
    link: &'a str,
    brand: branding::Brand,
}

#[allow(unused)]
fn send_email(
    name: String,
    token_id: Uuid,
    email: String,
    state: web::Data<AppState>,
    lang: String,
) -> actix_web::Result<()> {
    let link = format!(
        "{}/recovery?token={}&email={}",
        state.frontend_url, token_id, email
    );

    let (body, subject) = match lang.as_str() {
        "de" | "de-DE" => {
            let template = StartRecoveryDETemplate {
                name: &name,
                link: &link,
                brand: state.brand,
            };
            match template.render() {
                Ok(b) => (b, "Passwort Wiederherstellung"),
                Err(e) => {
                    log::error!("Failed to render German password recovery email template for user '{name}': {e}");
                    return Err(Error::InternalError.into());
                }
            }
        }
        _ => {
            let template = StartRecoveryENTemplate {
                name: &name,
                link: &link,
                brand: state.brand,
            };
            match template.render() {
                Ok(b) => (b, "Password Recovery"),
                Err(e) => {
                    log::error!("Failed to render English password recovery email template for user '{name}': {e}");
                    return Err(Error::InternalError.into());
                }
            }
        }
    };

    utils::send_email(&email, subject, body, &state);

    Ok(())
}

/// Start the process of account recovery.
#[utoipa::path(
    context_path="/auth",
    params(
        StartRecoveryQuery
    ),
    responses(
        (status = 200, description = "Request was successful"),
        (status = 500, description = "Internal server error"),
    )
)]
#[get("/start_recovery")]
pub async fn start_recovery(
    query: web::Query<StartRecoveryQuery>,
    state: web::Data<AppState>,
    rate_limiter: web::Data<LoginRateLimiter>,
    req: HttpRequest,
    #[cfg(not(test))] lang: crate::models::lang::Lang,
) -> actix_web::Result<impl Responder> {
    rate_limiter.check(query.email.to_lowercase(), &req)?;

    let user_id = match get_user_id(
        &state,
        crate::routes::auth::login::FindBy::Email(query.email.to_lowercase()),
    )
    .await
    {
        Ok(user_id) => user_id,
        Err(_) => return Ok(HttpResponse::Ok()),
    };

    #[allow(unused)]
    let user = get_user(&state, user_id).await?;

    let token_id = Uuid::new_v4();
    let token = RecoveryToken {
        id: token_id,
        user_id,
        created: chrono::Utc::now().timestamp(),
    };

    let mut conn = get_connection(&state)?;
    web_block_unpacked(move || {
        use db_connector::schema::recovery_tokens::dsl::*;

        match diesel::insert_into(recovery_tokens)
            .values(token)
            .execute(&mut conn)
        {
            Ok(_) => Ok(()),
            Err(_err) => Err(Error::InternalError),
        }
    })
    .await?;

    #[cfg(not(test))]
    std::thread::spawn(move || {
        let email = if let Some(email) = user.delivery_email {
            email
        } else {
            user.email
        };

        log::info!(
            "Sending password recovery email to '{}' for user '{}'",
            email,
            user.name
        );
        match send_email(
            user.name.clone(),
            token_id,
            email.clone(),
            state.clone(),
            lang.into(),
        ) {
            Ok(()) => {
                log::info!(
                    "Password recovery email sent successfully to '{}' for user '{}'",
                    email,
                    user.name
                );
            }
            Err(e) => {
                log::error!(
                    "Failed to send password recovery email to '{}' for user '{}': {:?}",
                    email,
                    user.name,
                    e
                );
            }
        }
    });

    Ok(HttpResponse::Ok())
}

#[cfg(test)]
pub mod tests {
    use actix_web::{
        test::{self, TestRequest},
        App,
    };
    use db_connector::{models::recovery_tokens::RecoveryToken, test_connection_pool};
    use diesel::prelude::*;
    use uuid::Uuid;

    use crate::{
        routes::user::tests::{get_test_uuid, TestUser},
        tests::configure,
    };

    use super::start_recovery;

    pub async fn start_test_recovery(mail: &str) -> Uuid {
        use db_connector::schema::recovery_tokens::dsl::*;

        let app = App::new().configure(configure).service(start_recovery);
        let app = test::init_service(app).await;

        let req = TestRequest::get()
            .uri(&format!("/start_recovery?email={mail}"))
            .insert_header(("X-Forwarded-For", "123.123.123.2"))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 200);

        let uid = get_test_uuid(mail).unwrap();
        let pool = test_connection_pool();
        let mut conn = pool.get().unwrap();
        let token: RecoveryToken = recovery_tokens
            .filter(user_id.eq(uid))
            .select(RecoveryToken::as_select())
            .get_result(&mut conn)
            .unwrap();

        token.id
    }

    #[actix_web::test]
    async fn valid_start_recovery() {
        use db_connector::schema::recovery_tokens::dsl::*;

        let (_user, mail) = TestUser::random().await;

        let app = App::new().configure(configure).service(start_recovery);
        let app = test::init_service(app).await;

        let req = TestRequest::get()
            .uri(&format!("/start_recovery?email={mail}"))
            .insert_header(("X-Forwarded-For", "123.123.123.2"))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 200);

        let uid = get_test_uuid(&mail).unwrap();
        let pool = test_connection_pool();
        let mut conn = pool.get().unwrap();
        let token: RecoveryToken = recovery_tokens
            .filter(user_id.eq(uid))
            .select(RecoveryToken::as_select())
            .get_result(&mut conn)
            .unwrap();
        diesel::delete(recovery_tokens.find(token.id))
            .execute(&mut conn)
            .unwrap();
    }

    #[actix_web::test]
    async fn test_start_recovery_rate_limited() {
        let (_user, mail) = TestUser::random().await;
        let (_user2, mail2) = TestUser::random().await;

        let app = App::new().configure(configure).service(start_recovery);
        let app = test::init_service(app).await;

        // Use a unique IP to avoid interference from other tests
        let ip = "10.99.99.1";

        // Exhaust the rate limit burst (5 in test mode)
        for _ in 0..5 {
            let req = TestRequest::get()
                .uri(&format!("/start_recovery?email={mail}"))
                .insert_header(("X-Forwarded-For", ip))
                .to_request();
            let resp = test::call_service(&app, req).await;
            assert_eq!(resp.status(), 200);
        }

        // Next request with same email+IP should be rate limited
        let req = TestRequest::get()
            .uri(&format!("/start_recovery?email={mail}"))
            .insert_header(("X-Forwarded-For", ip))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 429);

        // Same IP but different email should still work
        let req = TestRequest::get()
            .uri(&format!("/start_recovery?email={mail2}"))
            .insert_header(("X-Forwarded-For", ip))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 200);

        // Same email but different IP should still work
        let req = TestRequest::get()
            .uri(&format!("/start_recovery?email={mail}"))
            .insert_header(("X-Forwarded-For", "10.99.99.2"))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 200);

        // Clean up recovery tokens
        use db_connector::schema::recovery_tokens::dsl::*;
        let uid = get_test_uuid(&mail).unwrap();
        let uid2 = get_test_uuid(&mail2).unwrap();
        let pool = test_connection_pool();
        let mut conn = pool.get().unwrap();
        diesel::delete(recovery_tokens.filter(user_id.eq_any(vec![uid, uid2])))
            .execute(&mut conn)
            .unwrap();
    }
}
