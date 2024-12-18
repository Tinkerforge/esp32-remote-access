use actix_web::{get, web, HttpResponse, Responder};
use askama::Template;
use db_connector::models::recovery_tokens::RecoveryToken;
use diesel::prelude::*;
use lettre::{message::header::ContentType, Message, SmtpTransport, Transport};
use serde::Deserialize;
use utoipa::IntoParams;
use uuid::Uuid;

use crate::{
    error::Error,
    routes::user::{get_user, get_user_id},
    utils::{get_connection, web_block_unpacked},
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
}

#[derive(Template)]
#[template(path = "start_recovery_en.html")]
struct StartRecoveryENTemplate<'a> {
    name: &'a str,
    link: &'a str,
}

#[allow(unused)]
fn send_email(
    name: String,
    token_id: Uuid,
    email: String,
    mailer: SmtpTransport,
    frontend_url: String,
    lang: String,
) -> actix_web::Result<()> {
    let link = format!(
        "{}/recovery?token={}&email={}",
        frontend_url, token_id, email
    );

    let body = match lang.as_str() {
        "de" | "de-DE" => {
            let template = StartRecoveryDETemplate {
                name: &name,
                link: &link,
            };
            match template.render() {
                Ok(b) => b,
                Err(_err) => return Err(Error::InternalError.into()),
            }
        }
        _ => {
            let template = StartRecoveryENTemplate {
                name: &name,
                link: &link,
            };
            match template.render() {
                Ok(b) => b,
                Err(_err) => return Err(Error::InternalError.into()),
            }
        }
    };

    let mail = Message::builder()
        .from("Warp <warp@tinkerforge.com>".parse().unwrap())
        .to(email.parse().unwrap())
        .subject("Password Recovery")
        .header(ContentType::TEXT_HTML)
        .body(body)
        .unwrap();
    match mailer.send(&mail) {
        Ok(_) => log::debug!("Send password recovery mail was successful."),
        Err(err) => {
            log::error!("Failed to send: {}", err);
            return Err(Error::InternalError.into());
        }
    }

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
        (status = 400, description = "User does not exist")
    )
)]
#[get("/start_recovery")]
pub async fn start_recovery(
    query: web::Query<StartRecoveryQuery>,
    state: web::Data<AppState>,
    #[cfg(not(test))] lang: crate::models::lang::Lang,
) -> actix_web::Result<impl Responder> {
    let user_id = get_user_id(
        &state,
        crate::routes::auth::login::FindBy::Email(query.email.clone()),
    )
    .await?;

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

        send_email(
            user.name,
            token_id,
            email,
            state.mailer.clone(),
            state.frontend_url.clone(),
            lang.into(),
        )
        .ok();
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
            .uri(&format!("/start_recovery?email={}", mail))
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

        token.id
    }

    #[actix_web::test]
    async fn valid_start_recovery() {
        use db_connector::schema::recovery_tokens::dsl::*;

        let (_user, mail) = TestUser::random().await;

        let app = App::new().configure(configure).service(start_recovery);
        let app = test::init_service(app).await;

        let req = TestRequest::get()
            .uri(&format!("/start_recovery?email={}", mail))
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
}
