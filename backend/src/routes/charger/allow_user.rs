use actix_web::{put, web, HttpResponse, Responder};
use db_connector::models::allowed_users::AllowedUser;
use diesel::prelude::*;
use serde::{Deserialize, Serialize};

use crate::{
    error::Error,
    routes::{charger::charger_belongs_to_user, user::get_uuid_from_email},
    utils::{get_connection, web_block_unpacked},
    AppState,
};

#[derive(Debug, Deserialize, Serialize)]
struct AllowUserSchema {
    charger_id: String,
    user_mail: String,
}

#[put("/allow_user")]
pub async fn allow_user(
    state: web::Data<AppState>,
    uid: crate::models::uuid::Uuid,
    allow_user: web::Json<AllowUserSchema>,
) -> Result<impl Responder, actix_web::Error> {
    use crate::schema::allowed_users::dsl::*;

    println!("Test1");
    if !charger_belongs_to_user(&state, uid.into(), allow_user.charger_id.clone()).await? {
        return Err(Error::Unauthorized.into());
    }
    println!("Test2");

    let allowed_uuid = get_uuid_from_email(&state, allow_user.user_mail.clone()).await?;
    println!("Test3");
    let mut conn = get_connection(&state)?;
    web_block_unpacked(move || {
        let u = AllowedUser {
            id: uuid::Uuid::new_v4(),
            user: allowed_uuid,
            charger: allow_user.charger_id.clone(),
            is_owner: false,
        };

        match diesel::insert_into(allowed_users)
            .values(u)
            .execute(&mut conn)
        {
            Ok(_) => Ok(()),
            Err(_err) => Err(Error::InternalError),
        }
    })
    .await?;

    Ok(HttpResponse::Ok())
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::{cookie::Cookie, test, App};

    use crate::{middleware::jwt::JwtMiddleware, routes::user::tests::TestUser, tests::configure};

    #[actix_web::test]
    async fn test_allow_users() {
        let mail1 = "allow_user1@test.invalid";
        let mail2 = "allow_user2@test.invalid";

        let _user2 = TestUser::new(mail2).await;
        let mut user1 = TestUser::new(mail1).await;

        let charger = "allow_user_charger";
        let token = user1.login().await.to_string();
        user1.add_charger(charger).await;

        let app = App::new().configure(configure).wrap(JwtMiddleware).service(allow_user);
        let app = test::init_service(app).await;

        let allow = AllowUserSchema {
            charger_id: charger.to_string(),
            user_mail: mail2.to_string()
        };
        let req = test::TestRequest::put()
            .uri("/allow_user")
            .cookie(Cookie::new("access_token", token))
            .set_json(allow)
            .to_request();
        let resp = test::call_service(&app, req).await;

        assert!(resp.status().is_success());
    }

    #[actix_web::test]
    async fn test_allow_users_non_existing() {
        let mail1 = "allow_user_non_existing1@test.invalid";
        let mail2 = "allow_user_non_existing2@test.invalid";

        let mut user1 = TestUser::new(mail1).await;

        let charger = "allow_user_charger";
        let token = user1.login().await.to_string();
        user1.add_charger(charger).await;

        let app = App::new().configure(configure).wrap(JwtMiddleware).service(allow_user);
        let app = test::init_service(app).await;

        let allow = AllowUserSchema {
            charger_id: charger.to_string(),
            user_mail: mail2.to_string()
        };
        let req = test::TestRequest::put()
            .uri("/allow_user")
            .cookie(Cookie::new("access_token", token))
            .set_json(allow)
            .to_request();
        let resp = test::call_service(&app, req).await;

        assert!(resp.status().is_client_error());
    }
}
