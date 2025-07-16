use actix_web::{post, web, HttpResponse, Responder};
use base64::Engine;
use db_connector::models::authorization_tokens::AuthorizationToken;
use diesel::prelude::*;
use rand::RngCore;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::{
    error::Error,
    models::response_auth_token::ResponseAuthorizationToken,
    utils::{get_connection, web_block_unpacked},
    AppState,
};

#[derive(Serialize, Deserialize, ToSchema)]
pub struct CreateAuthorizationTokenSchema {
    use_once: bool,
    name: String,
}

#[utoipa::path(
    context_path = "/user",
    request_body = CreateAuthorizationTokenSchema,
    responses(
        (status = 201, body = ResponseAuthorizationToken),
    ),
    security(
        ("jwt" = [])
    )
)]
#[post("/create_authorization_token")]
pub async fn create_authorization_token(
    state: web::Data<AppState>,
    user_id: crate::models::uuid::Uuid,
    schema: web::Json<CreateAuthorizationTokenSchema>,
) -> actix_web::Result<impl Responder> {
    let id = uuid::Uuid::new_v4();
    let mut token = vec![0u8; 32];
    rand::rng().fill_bytes(&mut token);
    let token = base64::engine::general_purpose::STANDARD.encode(token);
    let created_at = chrono::Utc::now().naive_utc();
    let auth_token = AuthorizationToken {
        id,
        user_id: user_id.clone().into(),
        token: token.clone(),
        use_once: schema.use_once,
        name: schema.name.clone(),
        created_at,
        last_used_at: None,
    };

    let mut conn = get_connection(&state)?;
    web_block_unpacked(move || {
        use db_connector::schema::authorization_tokens::dsl as authorization_tokens;

        match diesel::insert_into(authorization_tokens::authorization_tokens)
            .values(&auth_token)
            .execute(&mut conn)
        {
            Ok(_) => Ok(()),
            Err(_err) => Err(Error::InternalError),
        }
    })
    .await?;

    let response = ResponseAuthorizationToken {
        id: id.to_string(),
        token,
        use_once: schema.use_once,
        name: schema.name.clone(),
        created_at: created_at.and_utc().timestamp(),
        last_used_at: None,
    };
    Ok(HttpResponse::Created().json(response))
}

#[cfg(test)]
pub mod tests {
    use actix_web::{
        cookie::Cookie,
        test::{self, TestRequest},
        App,
    };

    use crate::{
        middleware::jwt::JwtMiddleware, models::response_auth_token::ResponseAuthorizationToken,
        routes::user::tests::TestUser, tests::configure,
    };

    use super::{create_authorization_token, CreateAuthorizationTokenSchema};

    pub async fn create_test_auth_token(
        user: &TestUser,
        use_once: bool,
    ) -> ResponseAuthorizationToken {
        let token = user.access_token.as_ref().unwrap();

        let app = App::new()
            .configure(configure)
            .wrap(JwtMiddleware)
            .service(create_authorization_token);
        let app = test::init_service(app).await;

        println!("{use_once}");
        let req = TestRequest::post()
            .uri("/create_authorization_token")
            .cookie(Cookie::new("access_token", token))
            .set_json(CreateAuthorizationTokenSchema {
                use_once,
                name: "Test Token".to_string(),
            })
            .to_request();

        let resp: ResponseAuthorizationToken = test::call_and_read_body_json(&app, req).await;
        resp
    }

    #[actix_web::test]
    async fn test_authorization_token_creation() {
        let (mut user, _) = TestUser::random().await;
        let token = user.login().await;

        let app = App::new()
            .configure(configure)
            .wrap(JwtMiddleware)
            .service(create_authorization_token);
        let app = test::init_service(app).await;

        let req = TestRequest::post()
            .uri("/create_authorization_token")
            .cookie(Cookie::new("access_token", token))
            .set_json(CreateAuthorizationTokenSchema {
                use_once: true,
                name: "Test Token".to_string(),
            })
            .to_request();

        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 201);
    }
}
