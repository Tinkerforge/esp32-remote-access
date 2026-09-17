use std::sync::Mutex;

use actix_web::{get, web, HttpRequest, HttpResponse, Responder};
use db_connector::models::users::User;
use diesel::{result::Error::NotFound, ExpressionMethods, QueryDsl, SelectableHelper};
use diesel_async::RunQueryDsl as _;
use lru::LruCache;
use serde::Deserialize;
use utoipa::IntoParams;

use crate::{
    error::Error,
    rate_limit::LoginRateLimiter,
    utils::{generate_random_bytes, get_connection},
    AppState,
};

#[derive(Deserialize, IntoParams)]
pub struct GetSaltQuery {
    email: String,
}

/// Get the salt needed to derive the login-key.
#[utoipa::path(
    context_path = "/auth",
    responses(
        (status = 200, body = Vec<u32>),
        (status = 400, description = "User does not exist")
    ),
    params(
        GetSaltQuery
    )
)]
#[get("/get_login_salt")]
pub async fn get_login_salt(
    state: web::Data<AppState>,
    query: web::Query<GetSaltQuery>,
    cache: web::Data<Mutex<LruCache<String, Vec<u8>>>>,
    rate_limiter: web::Data<LoginRateLimiter>,
    req: HttpRequest,
) -> actix_web::Result<impl Responder> {
    use db_connector::schema::users::dsl::*;

    let mail = query.email.to_lowercase();
    rate_limiter.check(mail.clone(), &req)?;

    let salt: Vec<u8> = {
        let mut conn = get_connection(&state).await?;
        match users
            .filter(email.eq(&mail))
            .select(User::as_select())
            .get_result(&mut conn)
            .await
        {
            Ok(user) => user.login_salt,
            Err(NotFound) => cache
                .lock()
                .unwrap()
                .get_or_insert(mail, generate_random_bytes)
                .to_vec(),
            Err(_err) => return Err(Error::InternalError.into()),
        }
    };

    Ok(HttpResponse::Ok().json(salt))
}

#[cfg(test)]
pub mod tests {
    use actix_web::{
        test::{self, TestRequest},
        App,
    };
    use db_connector::{models::users::User, test_connection_pool};
    use diesel::{ExpressionMethods, QueryDsl, SelectableHelper};
    use diesel_async::RunQueryDsl as _AsyncRunQueryDsl;

    use crate::{routes::user::tests::TestUser, tests::configure};

    use super::get_login_salt;

    pub async fn get_test_login_salt(mail: &str) -> Vec<u8> {
        let app = App::new().configure(configure).service(get_login_salt);
        let app = test::init_service(app).await;

        let req = TestRequest::get()
            .uri(&format!("/get_login_salt?email={mail}"))
            .append_header(("X-Forwarded-For", "123.123.123.2"))
            .to_request();
        let resp = test::call_and_read_body_json(&app, req).await;

        resp
    }

    #[actix_web::test]
    async fn test_get_login_salt() {
        use db_connector::schema::users::dsl::*;

        let (mut user, mail) = TestUser::random().await;
        user.login().await;

        let app = App::new().configure(configure).service(get_login_salt);
        let app = test::init_service(app).await;

        let req = test::TestRequest::get()
            .uri(&format!("/get_login_salt?email={mail}"))
            .append_header(("X-Forwarded-For", "123.123.123.2"))
            .to_request();
        let resp = test::call_service(&app, req).await;
        println!("{}", resp.status());
        println!("{:?}", resp.response().body());
        assert!(resp.status().is_success());

        let pool = test_connection_pool();
        let mut conn = pool.get().await.unwrap();

        let user: User = users
            .filter(email.eq(mail))
            .select(User::as_select())
            .get_result(&mut conn)
            .await
            .unwrap();
        let resp: Vec<u8> = test::read_body_json(resp).await;
        assert_eq!(user.login_salt, resp);
    }

    #[actix_web::test]
    async fn test_nonexisting_user() {
        let app = App::new().configure(configure).service(get_login_salt);
        let app = test::init_service(app).await;

        let mail = format!("{}@example.invalid", uuid::Uuid::new_v4());

        let req = test::TestRequest::get()
            .uri(&format!("/get_login_salt?email={mail}"))
            .append_header(("X-Forwarded-For", "123.123.123.2"))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_success());

        let first_salt: Vec<u8> = test::read_body_json(resp).await;
        assert_eq!(first_salt.len(), 24);

        let req = test::TestRequest::get()
            .uri(&format!("/get_login_salt?email={mail}"))
            .append_header(("X-Forwarded-For", "123.123.123.2"))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_success());
        let second_salt: Vec<u8> = test::read_body_json(resp).await;
        assert_eq!(second_salt, first_salt);

        let mail = format!("{}@example.invalid", uuid::Uuid::new_v4());
        let req = test::TestRequest::get()
            .uri(&format!("/get_login_salt?email={mail}"))
            .append_header(("X-Forwarded-For", "123.123.123.2"))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_success());
        let second_salt: Vec<u8> = test::read_body_json(resp).await;
        assert_ne!(second_salt, first_salt);
    }
}
