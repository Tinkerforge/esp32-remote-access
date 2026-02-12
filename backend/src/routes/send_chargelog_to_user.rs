use actix_multipart::form::{json::Json as MpJson, tempfile::TempFile, MultipartForm};
use actix_web::{post, web, HttpRequest, HttpResponse, Responder};
use askama::Template;
use lettre::message::IntoBody;
use serde::{Deserialize, Serialize};
use std::io::Read;
use utoipa::ToSchema;

use crate::{
    AppState, branding, error::Error, rate_limit::ChargerRateLimiter, routes::{
        charger::{add::password_matches, user_is_allowed},
        user::get_user,
    }, udp_server::packet::ChargeLogSendMetadata, utils::{get_charger_from_db, parse_uuid, send_email_with_attachment}
};

#[derive(Serialize, Deserialize, ToSchema)]
pub struct SendChargelogMetadata {
    pub charger_uuid: String,
    pub password: String,
    pub user_uuid: String,
    pub filename: String,
    pub display_name: String,
    pub monthly_send: bool,
    pub lang: Option<String>,
}

#[derive(ToSchema, MultipartForm)]
pub struct SendChargelogSchema {
    #[schema(value_type = SendChargelogMetadata)]
    pub json: MpJson<SendChargelogMetadata>,
    #[schema(value_type = Vec<u8>, format = "binary", content_media_type = "application/octet-stream")]
    pub chargelog: TempFile,
}

#[derive(Template)]
#[template(path = "chargelog_de.html")]
struct ChargelogDETemplate<'a> {
    name: &'a str,
    month: &'a str,
    display_name: &'a str,
    monthly_send: bool,
    brand: branding::Brand,
}

#[derive(Template)]
#[template(path = "chargelog_en.html")]
struct ChargelogENTemplate<'a> {
    name: &'a str,
    month: &'a str,
    display_name: &'a str,
    monthly_send: bool,
    brand: branding::Brand,
}

fn render_chargelog_email(
    user_name: &str,
    month: &str,
    display_name: &str,
    lang: &str,
    monthly_send: bool,
    brand: branding::Brand,
) -> actix_web::Result<(String, String)> {
    let (body, subject) = match lang {
        "de" | "de-DE" => {
            let template = ChargelogDETemplate {
                name: user_name,
                month,
                display_name,
                monthly_send,
                brand,
            };
            match template.render() {
                Ok(b) => {
                    let subject = if monthly_send {
                        format!("Dein Ladelog für {} von {}", month, display_name)
                    } else {
                        format!("Dein Ladelog von {}", display_name)
                    };
                    (b, subject)
                }
                Err(e) => {
                    log::error!(
                        "Failed to render German chargelog email template for user '{}': {}",
                        user_name,
                        e
                    );
                    return Err(crate::error::Error::InternalError.into());
                }
            }
        }
        _ => {
            let template = ChargelogENTemplate {
                name: user_name,
                month,
                display_name,
                monthly_send,
                brand,
            };
            match template.render() {
                Ok(b) => {
                    let subject = if monthly_send {
                        format!("Your Charge Log for {} from {}", month, display_name)
                    } else {
                        format!("Your Charge Log from {}", display_name)
                    };
                    (b, subject)
                }
                Err(e) => {
                    log::error!(
                        "Failed to render English chargelog email template for user '{}': {}",
                        user_name,
                        e
                    );
                    return Err(crate::error::Error::InternalError.into());
                }
            }
        }
    };
    Ok((body, subject))
}

#[utoipa::path(
    request_body = SendChargelogSchema,
    responses(
        (status = 200, description = "Chargelog sent via email"),
        (status = 401, description = "Invalid charger credentials or rate limit exceeded"),
        (status = 500, description = "Internal server error"),
    )
)]
#[post("/send_chargelog_to_user")]
pub async fn send_chargelog(
    req: HttpRequest,
    state: web::Data<AppState>,
    rate_limiter: web::Data<ChargerRateLimiter>,
    form: MultipartForm<SendChargelogSchema>,
    #[cfg(not(test))] lang: crate::models::lang::Lang,
) -> actix_web::Result<impl Responder> {
    let SendChargelogSchema { json, chargelog } = form.into_inner();
    let metadata = json.into_inner();

    rate_limiter.check(metadata.charger_uuid.clone(), &req)?;

    let charger_id = parse_uuid(&metadata.charger_uuid)?;
    let charger = get_charger_from_db(charger_id, &state).await?;
    if !password_matches(&metadata.password, &charger.password, &state.hasher).await? {
        return Err(Error::ChargerCredentialsWrong.into());
    }

    let user = parse_uuid(&metadata.user_uuid)?;
    let user = get_user(&state, user).await?;

    #[cfg(not(test))]
    let lang_str: String = lang.into();
    #[cfg(test)]
    let lang_str = String::from("en");

    let Some(last_month) = chrono::Utc::now()
        .date_naive()
        .checked_sub_months(chrono::Months::new(1))
    else {
        return Err(Error::InternalError.into());
    };
    let month = match lang_str.as_str() {
        "de" => last_month
            .format_localized("%B %Y", chrono::Locale::de_DE)
            .to_string(),
        _ => last_month.format("%B %Y").to_string(),
    };

    let (body, subject) = render_chargelog_email(
        &user.name,
        &month,
        &metadata.display_name,
        &lang_str,
        metadata.monthly_send,
        state.brand,
    )?;

    let mut chargelog_file = chargelog.file.reopen().map_err(|err| {
        log::error!(
            "Failed to reopen chargelog temporary file '{}' for user '{}': {}",
            metadata.filename,
            user.email,
            err
        );
        Error::InternalError
    })?;
    let mut chargelog_bytes = Vec::with_capacity(chargelog.size);
    chargelog_file
        .read_to_end(&mut chargelog_bytes)
        .map_err(|err| {
            log::error!(
                "Failed to read chargelog temporary file '{}' for user '{}': {}",
                metadata.filename,
                user.email,
                err
            );
            Error::InternalError
        })?;

    send_email_with_attachment(
        &user.email,
        &subject,
        body,
        chargelog_bytes,
        &metadata.filename,
        &state,
    );

    Ok(HttpResponse::Ok())
}

/// Send a charge log to a user via email using data received over TCP.
///
/// This function takes a `ChargeLogSendMetadataPacket` containing metadata about the charge log
/// and a `BufReader` containing the charge log data. It verifies that the user specified in the
/// metadata is allowed to access the charger, then sends the charge log via email.
///
/// # Arguments
/// * `metadata` - The metadata packet containing charger UUID, user UUID, filename, and display name
/// * `reader` - A BufReader containing the charge log data
/// * `state` - The application state containing database pool and other configuration
///
/// # Returns
/// * `Ok(())` if the email was sent successfully
/// * `Err(...)` if the user is not allowed to access the charger or if any other error occurs
pub async fn send_charge_log_to_user<R: IntoBody>(
    charger_uuid: uuid::Uuid,
    metadata: &ChargeLogSendMetadata,
    charge_log: R,
    state: &web::Data<AppState>,
) -> Result<(), Error> {
    let user_uuid = uuid::Uuid::from_u128(metadata.user_uuid);

    // Check if the user is allowed to access this charger
    let is_allowed = user_is_allowed(state, user_uuid, charger_uuid)
        .await
        .map_err(|e| {
            log::error!(
                "Failed to check if user '{}' is allowed to access charger '{}': {:?}",
                user_uuid,
                charger_uuid,
                e
            );
            Error::InternalError
        })?;

    if !is_allowed {
        log::warn!(
            "User '{}' is not allowed to access charger '{}', not sending charge log",
            user_uuid,
            charger_uuid
        );
        return Err(Error::Unauthorized);
    }

    // Get the user from the database
    let user = get_user(state, user_uuid).await.map_err(|e| {
        log::error!("Failed to get user '{}': {:?}", user_uuid, e);
        Error::InternalError
    })?;

    // Use the language from the metadata packet
    let lang_str = &metadata.lang;
    let Some(last_month) = chrono::Utc::now()
        .date_naive()
        .checked_sub_months(chrono::Months::new(1))
    else {
        log::error!("Failed to calculate last month for charge log email");
        return Err(Error::InternalError);
    };
    let month = last_month.format("%B %Y").to_string();

    // Render the email template
    let (body, subject) = render_chargelog_email(
        &user.name,
        &month,
        &metadata.display_name,
        lang_str,
        metadata.is_monthly_email,
        state.brand,
    )
    .map_err(|e| {
        log::error!(
            "Failed to render charge log email for user '{}': {:?}",
            user.email,
            e
        );
        Error::InternalError
    })?;

    // Send the email with the charge log attached
    send_email_with_attachment(
        &user.email,
        &subject,
        body,
        charge_log,
        &metadata.filename,
        state,
    );

    log::error!(
        "Successfully sent charge log from charger '{}' to user '{}' ({})",
        charger_uuid,
        user_uuid,
        user.email
    );

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{routes::user::tests::TestUser, tests::configure};
    use actix_web::{test, App};
    use serde_json::{json, Value};

    fn build_multipart_body(boundary: &str, metadata: &Value, file_bytes: &[u8]) -> Vec<u8> {
        let metadata_str = metadata.to_string();
        let mut body = Vec::new();
        body.extend_from_slice(
            format!(
                "--{boundary}\r\nContent-Disposition: form-data; name=\"json\"\r\nContent-Type: application/json\r\n\r\n{}\r\n",
                metadata_str
            )
            .as_bytes(),
        );
        body.extend_from_slice(
            format!(
                "--{boundary}\r\nContent-Disposition: form-data; name=\"chargelog\"; filename=\"chargelog.pdf\"\r\nContent-Type: application/pdf\r\n\r\n"
            )
            .as_bytes(),
        );
        body.extend_from_slice(file_bytes);
        body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
        body
    }

    #[actix_web::test]
    async fn test_send_chargelog_success() {
        let (mut user, _mail) = TestUser::random().await;
        user.login().await;
        let charger = user.add_random_charger().await;

        let app = App::new().configure(configure).service(send_chargelog);
        let app = test::init_service(app).await;

        let metadata = json!({
            "charger_uuid": charger.uuid,
            "password": charger.password,
            "user_uuid": crate::routes::user::tests::get_test_uuid(&user.mail)
                .unwrap()
                .to_string(),
            "display_name": "Test Device",
            "filename": "chargelog.pdf",
            "monthly_send": true,
            "lang": "en"
        });

        let boundary = "----testboundary";
        let body = build_multipart_body(boundary, &metadata, &[1, 2, 3, 4, 5]);

        let req = test::TestRequest::post()
            .uri("/send_chargelog_to_user")
            .append_header(("X-Forwarded-For", "123.123.123.3"))
            .append_header((
                "Content-Type",
                format!("multipart/form-data; boundary={boundary}"),
            ))
            .set_payload(body)
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 200);
    }

    #[actix_web::test]
    async fn test_send_chargelog_invalid_password() {
        let (mut user, _mail) = TestUser::random().await;
        user.login().await;
        let charger = user.add_random_charger().await;

        let app = App::new().configure(configure).service(send_chargelog);
        let app = test::init_service(app).await;

        let metadata = json!({
            "charger_uuid": charger.uuid,
            "password": "wrongpassword",
            "user_uuid": crate::routes::user::tests::get_test_uuid(&user.mail)
                .unwrap()
                .to_string(),
            "display_name": "Test Device",
            "filename": "chargelog.pdf",
            "monthly_send": false,
            "lang": "de",
            "lang": "de"
        });

        let boundary = "----testboundary2";
        let body = build_multipart_body(boundary, &metadata, &[1, 2, 3, 4, 5]);

        let req = test::TestRequest::post()
            .uri("/send_chargelog_to_user")
            .append_header(("X-Forwarded-For", "123.123.123.3"))
            .append_header((
                "Content-Type",
                format!("multipart/form-data; boundary={boundary}"),
            ))
            .set_payload(body)
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 401);
    }

    #[actix_web::test]
    async fn test_send_chargelog_rate_limit() {
        let (mut user, _mail) = TestUser::random().await;
        user.login().await;
        let charger = user.add_random_charger().await;

        let app = App::new().configure(configure).service(send_chargelog);
        let app = test::init_service(app).await;

        let user_uuid = crate::routes::user::tests::get_test_uuid(&user.mail)
            .unwrap()
            .to_string();

        let metadata = json!({
            "charger_uuid": charger.uuid,
            "password": charger.password,
            "user_uuid": user_uuid,
            "display_name": "Test Device",
            "filename": "chargelog.pdf",
            "monthly_send": true,
            "lang": "en"
        });

        let boundary = "----testboundary3";
        let ip = "123.123.123.100";

        // Send requests up to the burst limit (5 in test mode)
        for i in 0..5 {
            let body = build_multipart_body(boundary, &metadata, &[1, 2, 3, 4, 5]);
            let req = test::TestRequest::post()
                .uri("/send_chargelog_to_user")
                .append_header(("X-Forwarded-For", ip))
                .append_header((
                    "Content-Type",
                    format!("multipart/form-data; boundary={boundary}"),
                ))
                .set_payload(body)
                .to_request();
            let resp = test::call_service(&app, req).await;
            assert_eq!(
                resp.status(),
                200,
                "Request {} should succeed (within burst limit)",
                i + 1
            );
        }

        // The 6th request should be rate limited
        let body = build_multipart_body(boundary, &metadata, &[1, 2, 3, 4, 5]);
        let req = test::TestRequest::post()
            .uri("/send_chargelog_to_user")
            .append_header(("X-Forwarded-For", ip))
            .append_header((
                "Content-Type",
                format!("multipart/form-data; boundary={boundary}"),
            ))
            .set_payload(body)
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(
            resp.status(),
            429,
            "Request 6 should be rate limited (429 Too Many Requests)"
        );

        // Verify that a different IP can still make requests
        let body = build_multipart_body(boundary, &metadata, &[1, 2, 3, 4, 5]);
        let req = test::TestRequest::post()
            .uri("/send_chargelog_to_user")
            .append_header(("X-Forwarded-For", "123.123.123.101"))
            .append_header((
                "Content-Type",
                format!("multipart/form-data; boundary={boundary}"),
            ))
            .set_payload(body)
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(
            resp.status(),
            200,
            "Request from different IP should succeed"
        );

        // Verify that a different charger from the same IP can make requests
        let charger2 = user.add_random_charger().await;
        let metadata2 = json!({
            "charger_uuid": charger2.uuid,
            "password": charger2.password,
            "user_uuid": user_uuid,
            "display_name": "Test Device 2",
            "filename": "chargelog2.pdf",
            "monthly_send": false,
            "lang": "de"
        });
        let body = build_multipart_body(boundary, &metadata2, &[1, 2, 3, 4, 5]);
        let req = test::TestRequest::post()
            .uri("/send_chargelog_to_user")
            .append_header(("X-Forwarded-For", ip))
            .append_header((
                "Content-Type",
                format!("multipart/form-data; boundary={boundary}"),
            ))
            .set_payload(body)
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(
            resp.status(),
            200,
            "Request from different charger (same IP) should succeed"
        );
    }
}
