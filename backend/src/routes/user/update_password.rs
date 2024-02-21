use actix_web::{put, web, HttpResponse, Responder};
use diesel::prelude::*;
use serde::{Deserialize, Serialize};
use validator::Validate;

use crate::{
    error::Error,
    routes::auth::{
        login::{validate_password, FindBy},
        register::hash_pass,
    },
    utils::get_connection,
    AppState,
};

#[derive(Validate, Deserialize, Serialize)]
struct PasswordUpdate {
    old_pass: String,
    #[validate(length(min = 12))]
    new_pass: String,
}

#[put("/update_password")]
pub async fn update_password(
    state: web::Data<AppState>,
    uid: crate::models::uuid::Uuid,
    data: actix_web_validator::Json<PasswordUpdate>,
) -> Result<impl Responder, actix_web::Error> {
    use crate::schema::users::dsl::*;

    let conn = get_connection(&state)?;
    let _ = validate_password(&data.old_pass, FindBy::Uuid(uid.clone().into()), conn).await?;

    let new_hash = match hash_pass(&data.new_pass) {
        Ok(hash) => hash,
        Err(_err) => return Err(Error::InternalError.into()),
    };

    let mut conn = get_connection(&state)?;
    match web::block(move || {
        match diesel::update(users.find::<uuid::Uuid>(uid.into()))
            .set(password.eq(new_hash))
            .execute(&mut conn)
        {
            Ok(_) => (),
            Err(_err) => return Err(Error::InternalError),
        }

        Ok(())
    })
    .await
    {
        Ok(res) => match res {
            Ok(()) => Ok(HttpResponse::Ok()),
            Err(err) => Err(err.into()),
        },
        Err(_err) => Err(Error::InternalError.into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::{cookie::Cookie, test, App};

    use crate::{
        defer,
        routes::auth::{
            login::tests::{login_user, verify_and_login_user},
            register::tests::{create_user, delete_user},
        },
        tests::configure,
    };

    #[actix_web::test]
    async fn test_valid_password_update() {
        let mail = "valid_password_update@test.invalid";
        create_user(mail).await;
        defer!(delete_user(mail));

        let app = App::new()
            .configure(configure)
            .service(update_password)
            .wrap(crate::middleware::jwt::JwtMiddleware);
        let app = test::init_service(app).await;

        let new_pass = "TestTestTest1".to_string();
        let data = PasswordUpdate {
            old_pass: "TestTestTest".to_string(),
            new_pass: new_pass.clone(),
        };

        let token = verify_and_login_user(mail).await;
        let req = test::TestRequest::put()
            .uri("/update_password")
            .cookie(Cookie::new("access_token", token))
            .set_json(data)
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_success());

        let _ = login_user(mail, Some(new_pass)).await;
    }

    #[actix_web::test]
    async fn test_invalid_old_password() {
        let mail = "invalid_password_update@test.invalid";
        create_user(mail).await;
        defer!(delete_user(mail));

        let app = App::new()
            .configure(configure)
            .service(update_password)
            .wrap(crate::middleware::jwt::JwtMiddleware);
        let app = test::init_service(app).await;

        let new_pass = "TestTestTest1".to_string();
        let data = PasswordUpdate {
            old_pass: "TestTestTest2".to_string(),
            new_pass: new_pass.clone(),
        };

        let token = verify_and_login_user(mail).await;
        let req = test::TestRequest::put()
            .uri("/update_password")
            .cookie(Cookie::new("access_token", token))
            .set_json(data)
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_client_error());

        let pool = db_connector::test_connection_pool();
        let conn = pool.get().unwrap();
        assert!(
            validate_password(&new_pass, FindBy::Email(mail.to_string()), conn)
                .await
                .is_err()
        );
    }
}
