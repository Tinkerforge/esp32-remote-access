use actix_web::{get, web, HttpResponse, Responder};
use db_connector::models::authorization_tokens::AuthorizationToken;
use diesel::prelude::*;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::{
    error::Error,
    models::response_auth_token::ResponseAuthorizationToken,
    utils::{get_connection, web_block_unpacked},
    AppState,
};

#[derive(Serialize, Deserialize, ToSchema)]
pub struct GetAuthorizationTokensResponseSchema {
    tokens: Vec<ResponseAuthorizationToken>,
}

#[utoipa::path(
    context_path = "/user",
    responses(
        (status = 200, body = GetAuthorizationTokensResponseSchema),
    ),
    security(
        ("jwt" = [])
    )
)]
#[get("/get_authorization_tokens")]
pub async fn get_authorization_tokens(
    state: web::Data<AppState>,
    user_id: crate::models::uuid::Uuid,
) -> actix_web::Result<impl Responder> {
    let mut conn = get_connection(&state)?;
    let user_tokens: Vec<AuthorizationToken> = web_block_unpacked(move || {
        use db_connector::schema::authorization_tokens::dsl as authorization_tokens;

        let user_id: uuid::Uuid = user_id.into();
        match authorization_tokens::authorization_tokens
            .filter(authorization_tokens::user_id.eq(&user_id))
            .select(AuthorizationToken::as_select())
            .load(&mut conn)
        {
            Ok(u) => Ok(u),
            Err(_err) => Err(Error::InternalError),
        }
    })
    .await?;

    let tokens: Vec<ResponseAuthorizationToken> = user_tokens
        .into_iter()
        .map(|t| ResponseAuthorizationToken {
            id: t.id.to_string(),
            token: t.token,
            use_once: t.use_once,
            name: t.name,
            created_at: t.created_at.and_utc().timestamp(),
            last_used_at: t.last_used_at.map(|dt| dt.and_utc().timestamp()),
        })
        .collect();

    Ok(HttpResponse::Ok().json(GetAuthorizationTokensResponseSchema { tokens }))
}

#[cfg(test)]
mod tests {
    use actix_web::{cookie::Cookie, test, App};

    use crate::{
        middleware::jwt::JwtMiddleware, models::response_auth_token::ResponseAuthorizationToken,
        routes::user::tests::TestUser, tests::configure,
    };

    use super::{get_authorization_tokens, GetAuthorizationTokensResponseSchema};

    #[actix_web::test]
    async fn test_get_authorization_tokens() {
        let (mut user, _) = TestUser::random().await;
        let access_token = user.login().await.to_string();

        let mut auth_tokens = vec![
            ResponseAuthorizationToken {
                id: String::new(),
                token: String::new(),
                use_once: false,
                name:String::new(),
                created_at: 0,
                last_used_at: None,
            };
            5
        ];
        for token in auth_tokens.iter_mut() {
            *token = user.create_authorization_token(true).await;
        }

        let app = App::new()
            .configure(configure)
            .wrap(JwtMiddleware)
            .service(get_authorization_tokens);
        let app = test::init_service(app).await;

        let req = test::TestRequest::get()
            .uri("/get_authorization_tokens")
            .cookie(Cookie::new("access_token", access_token))
            .to_request();

        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 200);
        let resp: GetAuthorizationTokensResponseSchema = test::read_body_json(resp).await;
        assert_eq!(auth_tokens, resp.tokens);
    }
}
