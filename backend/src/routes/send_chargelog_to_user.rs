use actix_web::{post, web, HttpRequest, HttpResponse, Responder};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::{
    error::Error,
    rate_limit::ChargerRateLimiter,
    routes::charger::add::{get_charger_from_db, password_matches},
    utils::{parse_uuid, send_email_with_attachment},
    AppState,
};

#[derive(Serialize, Deserialize, ToSchema)]
pub struct SendChargelogSchema {
    pub charger_uuid: String,
    pub password: String,
    pub user_email: String,
    pub chargelog: Vec<u8>, // binary data
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
    payload: web::Json<SendChargelogSchema>,
) -> actix_web::Result<impl Responder> {
    rate_limiter.check(payload.charger_uuid.clone(), &req)?;

    let charger_id = parse_uuid(&payload.charger_uuid)?;
    let charger = get_charger_from_db(charger_id, &state).await?;
    if !password_matches(&payload.password, &charger.password)? {
        return Err(Error::ChargerCredentialsWrong.into());
    }

    let subject = "Your Charger Log";
    let body = "Attached is your requested chargelog.".to_string();
    send_email_with_attachment(
        &payload.user_email,
        subject,
        body,
        payload.chargelog.clone(),
        "chargelog.bin",
        &state,
    );

    Ok(HttpResponse::Ok())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{routes::user::tests::TestUser, tests::configure};
    use actix_web::{test, App};

    #[actix_web::test]
    async fn test_send_chargelog_success() {
        let (mut user, _mail) = TestUser::random().await;
        user.login().await;
        let charger = user.add_random_charger().await;

        let app = App::new().configure(configure).service(send_chargelog);
        let app = test::init_service(app).await;

        let payload = SendChargelogSchema {
            charger_uuid: charger.uuid.clone(),
            password: charger.password.clone(),
            user_email: user.mail.clone(),
            chargelog: vec![1, 2, 3, 4, 5],
        };

        let req = test::TestRequest::post()
            .uri("/send_chargelog_to_user")
            .append_header(("X-Forwarded-For", "123.123.123.3"))
            .set_json(payload)
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

        let payload = SendChargelogSchema {
            charger_uuid: charger.uuid.clone(),
            password: "wrongpassword".to_string(),
            user_email: user.mail.clone(),
            chargelog: vec![1, 2, 3, 4, 5],
        };

        let req = test::TestRequest::post()
            .uri("/send_chargelog_to_user")
            .append_header(("X-Forwarded-For", "123.123.123.3"))
            .set_json(payload)
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 401);
    }
}
