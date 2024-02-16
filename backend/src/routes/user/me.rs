use actix_web::{get, web, HttpResponse, Responder};

use crate::{models::{filtered_user::FilteredUser, uuid}, routes::user::get_user, AppState};

#[get("/me")]
async fn me(state: web::Data<AppState>, id: uuid::Uuid) -> Result<impl Responder, actix_web::Error> {
    let user = get_user(&state, id.into()).await?;

    Ok(HttpResponse::Ok().json(FilteredUser::from(user)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::{cookie::Cookie, test, App};

    use crate::{defer, routes::auth::{login::tests::verify_and_login_user, register::tests::{create_user, delete_user}}, tests::configure};

    #[actix_web::test]
    async fn test_me() {
        let mail = "me@test.invalid";
        create_user(mail).await;
        defer!(delete_user(mail));

        let app = App::new().configure(configure ).service(me)
            .wrap(crate::middleware::jwt::JwtMiddleware);
        let app = test::init_service(app).await;

        let token = verify_and_login_user(mail).await;
        let req = test::TestRequest::get()
            .uri("/me")
            .cookie(Cookie::new("access_token", token))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_success());

        let body: FilteredUser = test::read_body_json(resp).await;
        assert_eq!(body.email, mail);
    }
}
