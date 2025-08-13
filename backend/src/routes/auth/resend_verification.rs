use actix_web::{post, web, HttpResponse, Responder};
use askama::Template;
use chrono::Days;
use diesel::prelude::*;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::{error::Error, routes::auth::VERIFICATION_EXPIRATION_DAYS, utils::{get_connection, web_block_unpacked}, AppState};

use db_connector::models::{users::User, verification::Verification};

#[derive(Template)]
#[template(path = "email_verification_en.html")]
pub struct VerifyEmailENTemplate<'a> {
    pub name: &'a str,
    pub link: &'a str,
}

#[derive(Template)]
#[template(path = "email_verification_de.html")]
pub struct VerifyEmailDETemplate<'a> {
    pub name: &'a str,
    pub link: &'a str,
}

#[derive(Deserialize, ToSchema, Serialize)]
pub struct ResendSchema {
    pub email: String,
}

#[allow(unused)]
fn send_verification_mail(
    name: String,
    id: Verification,
    email: String,
    state: web::Data<AppState>,
    lang: String,
) -> Result<(), actix_web::Error> {
    let link = format!("{}/api/auth/verify?id={}", state.frontend_url, id.id);

    let (body, subject) = match lang.as_str() {
        "de" | "de-DE" => {
            let template = VerifyEmailDETemplate { name: &name, link: &link };
            match template.render() { Ok(body) => (body, "Email verifizieren"), Err(e) => { log::error!("Failed to render German verification email template for user '{name}': {e}"); return Err(Error::InternalError.into()); } }
        }
        _ => {
            let template = VerifyEmailENTemplate { name: &name, link: &link };
            match template.render() { Ok(body) => (body, "Verify email"), Err(e) => { log::error!("Failed to render English verification email template for user '{name}': {e}"); return Err(Error::InternalError.into()); } }
        }
    };

    crate::utils::send_email(&email, subject, body, &state);
    Ok(())
}

/// Resend a verification email if user exists and not verified yet.
#[utoipa::path(
    context_path = "/auth",
    responses(
        (status = 200, description = "Verification email resent (or already verified but hidden)."),
        (status = 404, description = "User not found")
    )
)]
#[post("/resend_verification")]
pub async fn resend_verification(
    state: web::Data<AppState>,
    data: web::Json<ResendSchema>,
    #[cfg(not(test))] lang: crate::models::lang::Lang,
) -> actix_web::Result<impl Responder> {
    use db_connector::schema::users::dsl as u_dsl;


    let mut conn = get_connection(&state)?;
    let user_email = data.email.to_lowercase();

    // Load user
    let db_user: User = web_block_unpacked(move || {
        match u_dsl::users
            .filter(u_dsl::email.eq(&user_email))
            .select(User::as_select())
            .get_result(&mut conn) {
                Ok(u) => Ok(u),
                Err(diesel::result::Error::NotFound) => Err(Error::UserDoesNotExist),
                Err(_) => Err(Error::InternalError)
            }
    }).await?;

    if db_user.email_verified { // silently return success
        return Ok(HttpResponse::Ok());
    }

    let mut conn = get_connection(&state)?;

    // (Re)create verification token (delete old first if present)
    let user_id = db_user.id;
    web_block_unpacked(move || {
        use db_connector::schema::verification::dsl::*;
        // remove old tokens
        let _ = diesel::delete(verification.filter(user.eq(user_id))).execute(&mut conn);

        let exp = chrono::Utc::now().checked_add_days(Days::new(VERIFICATION_EXPIRATION_DAYS)).ok_or(Error::InternalError)?;

        let verify = Verification { id: uuid::Uuid::new_v4(), user: user_id, expiration: exp.naive_utc() };
        diesel::insert_into(verification).values(&verify).execute(&mut conn).map_err(|_| Error::InternalError)?;
        Ok(verify)
    }).await.and_then(|_verify| {
        #[cfg(not(test))]
        {
            let user_name = db_user.name.clone();
            let lang: String = lang.into();
            let state_cpy = state.clone();
            let email_cpy = data.email.clone();
            std::thread::spawn(move || {
                if let Err(e) = send_verification_mail(user_name, _verify, email_cpy, state_cpy, lang) {
                    log::error!("Failed to resend verification mail: {e:?}");
                }
            });
        }
        Ok(())
    })?;

    Ok(HttpResponse::Ok())
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::{test, App};
    use crate::tests::configure;
    use crate::routes::auth::register::tests::{create_user, delete_user};
    use crate::routes::auth::verify::tests::fast_verify;

    #[actix_web::test]
    async fn test_resend_unverified() {
        let mail = "resend_unverified@test.invalid";
        create_user(mail).await;
        let app = App::new().configure(configure).service(resend_verification);
        let app = test::init_service(app).await;
        let req = test::TestRequest::post().uri("/resend_verification").set_json(&ResendSchema{ email: mail.to_string() }).to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_success());
        delete_user(mail);
    }

    #[actix_web::test]
    async fn test_resend_verified() {
        let mail = "resend_verified@test.invalid";
        create_user(mail).await;
        fast_verify(mail);
        let app = App::new().configure(configure).service(resend_verification);
        let app = test::init_service(app).await;
        let req = test::TestRequest::post().uri("/resend_verification").set_json(&ResendSchema{ email: mail.to_string() }).to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_success());
        delete_user(mail);
    }

    #[actix_web::test]
    async fn test_resend_missing() {
        let mail = "resend_missing@test.invalid";
        let app = App::new().configure(configure).service(resend_verification);
        let app = test::init_service(app).await;
        let req = test::TestRequest::post().uri("/resend_verification").set_json(&ResendSchema{ email: mail.to_string() }).to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status().as_u16(), 400); // mapped from UserDoesNotExist
    }
}
