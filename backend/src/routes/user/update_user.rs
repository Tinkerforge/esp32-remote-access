use actix_web::{put, web, HttpResponse, Responder};
use diesel::{prelude::*, result::Error::NotFound};

use crate::{error::Error, models::filtered_user::FilteredUser, utils::get_connection, AppState};

#[put("/update_user")]
pub async fn update_user(
    state: web::Data<AppState>,
    user: actix_web_validator::Json<FilteredUser>,
    uid: crate::models::uuid::Uuid) -> Result<impl Responder, actix_web::Error>
{
    use crate::schema::users::dsl::*;

    let mut conn = get_connection(&state)?;
    match web::block(move || {
        match diesel::update(users.find::<uuid::Uuid>(uid.clone().into())).set(email.eq(&user.email)).execute(&mut conn) {
            Ok(_) => (),
            Err(NotFound) => return Err(Error::Unauthorized),
            Err(_err) => {
                return Err(Error::InternalError)
            }
        }

        match diesel::update(users.find::<uuid::Uuid>(uid.into())).set(name.eq(&user.name)).execute(&mut conn) {
            Ok(_) => (),
            Err(NotFound) => return Err(Error::Unauthorized),
            Err(_err) => {
                return Err(Error::InternalError)
            }
        }

        Ok(())
    }).await {
        Ok(res) => match res {
            Ok(()) => (),
            Err(err) => return Err(err.into())
        },
        Err(_err) => {
            return Err(Error::InternalError.into())
        }
    }

    Ok(HttpResponse::Ok())
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::{cookie::Cookie, test, App};
    use crate::{defer, routes::{auth::{login::tests::verify_and_login_user, register::tests::{create_user, delete_user}}, user::me::tests::get_test_user}, tests::configure};


    #[actix_web::test]
    async fn test_update_email() {
        let mail = "update_mail@test.invalid";
        create_user(mail).await;
        defer!(delete_user(mail));
        let update_mail = format!("t{}", mail);
        defer!(delete_user(&update_mail));

        let app = App::new().configure(configure ).service(update_user)
            .wrap(crate::middleware::jwt::JwtMiddleware);
        let app = test::init_service(app).await;

        let user = get_test_user(mail);
        let mut user = FilteredUser::from(user);
        user.email = update_mail.clone();

        let token = verify_and_login_user(mail).await;
        let req = test::TestRequest::put()
            .uri("/update_user")
            .set_json(user)
            .cookie(Cookie::new("access_token", token))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_success());

        let _ = get_test_user(&update_mail);
    }
}
