use actix_web::{get, web, HttpResponse, Responder};
use db_connector::models::users::User;
use diesel::{prelude::*, result::Error::NotFound};
use serde::Deserialize;
use utoipa::IntoParams;

use crate::{
    error::Error,
    utils::{get_connection, web_block_unpacked},
    AppState,
};

#[derive(Deserialize, IntoParams)]
pub struct GetSaltQuery {
    username: String,
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
) -> actix_web::Result<impl Responder> {
    use db_connector::schema::users::dsl::*;

    let mut conn = get_connection(&state)?;
    let user: User = web_block_unpacked(move || {
        match users
            .filter(name.eq(&query.username))
            .select(User::as_select())
            .get_result(&mut conn)
        {
            Ok(user) => Ok(user),
            Err(NotFound) => Err(Error::UserDoesNotExist),
            Err(_err) => Err(Error::InternalError),
        }
    })
    .await?;

    Ok(HttpResponse::Ok().json(user.login_salt))
}

#[cfg(test)]
mod tests {
    use actix_web::{test, App};
    use db_connector::{models::users::User, test_connection_pool};
    use diesel::prelude::*;

    use crate::{routes::user::tests::TestUser, tests::configure};

    use super::get_login_salt;

    #[actix_web::test]
    async fn test_get_login_salt() {
        use db_connector::schema::users::dsl::*;

        let (mut user, username) = TestUser::random().await;
        user.login().await;

        let app = App::new().configure(configure).service(get_login_salt);
        let app = test::init_service(app).await;

        let req = test::TestRequest::get()
            .uri(&format!("/get_login_salt?username={}", username))
            .to_request();
        let resp = test::call_service(&app, req).await;
        println!("{}", resp.status());
        println!("{:?}", resp.response().body());
        assert!(resp.status().is_success());

        let pool = test_connection_pool();
        let mut conn = pool.get().unwrap();

        let user: User = users
            .filter(name.eq(username))
            .select(User::as_select())
            .get_result(&mut conn)
            .unwrap();
        let resp: Vec<u8> = test::read_body_json(resp).await;
        assert_eq!(user.login_salt, resp);
    }
}
