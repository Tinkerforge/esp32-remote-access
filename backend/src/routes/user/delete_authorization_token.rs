use actix_web::{delete, web, HttpResponse, Responder};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use diesel::prelude::*;

use crate::{error::Error, utils::{get_connection, parse_uuid, web_block_unpacked}, AppState};

#[derive(Deserialize, Serialize, ToSchema)]
pub struct DeleteAuthorizationTokenSchema {
    id: String,
}

#[utoipa::path(
    context_path = "/user",
    request_body = DeleteAuthorizationTokenSchema,
    responses(
        (status = 200),
    ),
    security(
        ("jwt" = [])
    )
)]
#[delete("/delete_authorization_token")]
pub async fn delete_authorization_token(
    state: web::Data<AppState>,
    payload: web::Json<DeleteAuthorizationTokenSchema>,
    user_id: crate::models::uuid::Uuid,
) -> actix_web::Result<impl Responder>
{
    let auth_token_id = parse_uuid(&payload.id)?;
    let user_id: uuid::Uuid = user_id.into();

    let mut conn = get_connection(&state)?;
    web_block_unpacked(move || {
        use db_connector::schema::authorization_tokens::dsl as  authorization_tokens;
        match diesel::delete(authorization_tokens::authorization_tokens
            .filter(authorization_tokens::id.eq(auth_token_id))
            .filter(authorization_tokens::user_id.eq(user_id)))
            .execute(&mut conn)
        {
            Ok(_) => Ok(()),
            Err(_err) => Err(Error::InternalError)
        }
    }).await?;

    Ok(HttpResponse::Ok())
}

#[cfg(test)]
mod tests {
    use actix_web::{cookie::Cookie, test::{self, TestRequest}, App};
    use db_connector::{models::authorization_tokens::AuthorizationToken, test_connection_pool};
    use diesel::prelude::*;

    use crate::{middleware::jwt::JwtMiddleware, routes::user::{delete_authorization_token::DeleteAuthorizationTokenSchema, tests::TestUser}, tests::configure, utils::parse_uuid};

    use super::delete_authorization_token;


    #[actix_web::test]
    async fn test_delete_authorization_token() {
        let (mut user, _) = TestUser::random().await;
        let access_token = user.login().await.to_string();

        let auth_token = user.create_authorization_token(true).await;

        let app = App::new().configure(configure)
            .wrap(JwtMiddleware)
            .service(delete_authorization_token);
        let app = test::init_service(app).await;

        let body = DeleteAuthorizationTokenSchema {
            id: auth_token.id.to_string(),
        };

        let req = TestRequest::delete()
            .uri("/delete_authorization_token")
            .cookie(Cookie::new("access_token", access_token))
            .set_json(body)
            .to_request();

        let resp = test::call_service(&app, req).await;

        assert_eq!(resp.status(), 200);


        let pool = test_connection_pool();
        let mut conn = pool.get().unwrap();
        {
            use  db_connector::schema::authorization_tokens::dsl as  authorization_tokens;

            let id = parse_uuid(&auth_token.id).unwrap();
            let auth_token: Vec<AuthorizationToken> = authorization_tokens::authorization_tokens
                .filter(authorization_tokens::id.eq(&id))
                .load(&mut conn)
                .unwrap();

            assert_eq!(auth_token, vec![]);
        }
    }
}
