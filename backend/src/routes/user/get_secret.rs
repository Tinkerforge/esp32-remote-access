use actix_web::{get, web, HttpResponse, Responder};
use db_connector::models::users::User;
use diesel::{prelude::*, result::Error::NotFound};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::{
    error::Error,
    utils::{get_connection, web_block_unpacked},
    AppState,
};

#[derive(Serialize, Deserialize, ToSchema)]
pub struct GetSecretResponse {
    #[schema(value_type = Vec<u32>)]
    pub secret: Vec<u8>,
    #[schema(value_type = Vec<u32>)]
    pub secret_salt: Vec<u8>,
    #[schema(value_type = Vec<u32>)]
    pub secret_nonce: Vec<u8>,
}

#[utoipa::path(
    context_path = "/user",
    responses(
        (status = 200, body = GetSecretResponse),
    )
)]
#[get("/get_secret")]
pub async fn get_secret(
    state: web::Data<AppState>,
    uid: crate::models::uuid::Uuid,
) -> actix_web::Result<impl Responder> {
    use db_connector::schema::users::dsl as users;

    let mut conn = get_connection(&state)?;
    let response: GetSecretResponse = web_block_unpacked(move || {
        let uid: uuid::Uuid = uid.into();
        let user: User = match users::users
            .find(uid)
            .select(User::as_select())
            .get_result(&mut conn)
        {
            Ok(user) => user,
            Err(NotFound) => return Err(Error::UserDoesNotExist),
            Err(_err) => return Err(Error::InternalError),
        };
        Ok(GetSecretResponse {
            secret: user.secret,
            secret_nonce: user.secret_nonce,
            secret_salt: user.secret_salt,
        })
    })
    .await?;

    Ok(HttpResponse::Ok().json(response))
}

#[cfg(test)]
pub mod tests {
    use crate::{
        middleware::jwt::JwtMiddleware,
        routes::user::{
            get_secret::{get_secret, GetSecretResponse},
            tests::TestUser,
        },
        tests::configure,
    };

    use actix_web::{cookie::Cookie, test::{self, TestRequest}, App};
    use db_connector::{models::users::User, test_connection_pool};
    use diesel::prelude::*;

    pub async fn get_test_secret(access_token: &str) -> GetSecretResponse {
        let app = App::new().configure(configure).wrap(JwtMiddleware).service(get_secret);
        let app = test::init_service(app).await;

        let req = TestRequest::get()
            .uri("/get_secret")
            .cookie(Cookie::new("access_token", access_token))
            .to_request();

        let resp = test::call_and_read_body_json(&app, req).await;

        resp
    }

    #[actix_web::test]
    async fn test_get_secret() {
        use db_connector::schema::users::dsl::*;
        let (mut user, mail) = TestUser::random().await;
        let token = user.login().await.to_owned();

        let app = App::new()
            .configure(configure)
            .wrap(JwtMiddleware)
            .service(get_secret);
        let app = test::init_service(app).await;

        let req = test::TestRequest::get()
            .cookie(Cookie::new("access_token", token))
            .uri("/get_secret")
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_success());

        let pool = test_connection_pool();
        let mut conn = pool.get().unwrap();
        let user: User = users
            .filter(email.eq(mail))
            .select(User::as_select())
            .get_result(&mut conn)
            .unwrap();
        let resp: GetSecretResponse = test::read_body_json(resp).await;
        assert_eq!(user.secret, resp.secret);
        assert_eq!(user.secret_salt, resp.secret_salt);
    }

    #[actix_web::test]
    async fn test_two_existing_users() {
        use db_connector::schema::users::dsl::*;
        let (mut user, mail) = TestUser::random().await;
        let (mut user2, _) = TestUser::random().await;
        user2.login().await;
        let token = user.login().await.to_owned();

        let app = App::new()
            .configure(configure)
            .wrap(JwtMiddleware)
            .service(get_secret);
        let app = test::init_service(app).await;

        let req = test::TestRequest::get()
            .cookie(Cookie::new("access_token", token))
            .uri("/get_secret")
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_success());

        let pool = test_connection_pool();
        let mut conn = pool.get().unwrap();
        let user: User = users
            .filter(email.eq(mail))
            .select(User::as_select())
            .get_result(&mut conn)
            .unwrap();
        let resp: GetSecretResponse = test::read_body_json(resp).await;
        assert_eq!(user.secret, resp.secret);
        assert_eq!(user.secret_salt, resp.secret_salt);
    }
}
