use actix_web::{delete, web, HttpResponse, Responder};
use diesel::prelude::*;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::{
    error::Error,
    routes::charger::charger_belongs_to_user,
    utils::{get_connection, web_block_unpacked},
    AppState,
};

#[derive(Debug, Deserialize, Serialize, ToSchema)]
pub struct DeleteChargerSchema {
    charger: String,
}

async fn delete_all_keys(cid: String, state: &web::Data<AppState>) -> Result<(), actix_web::Error> {
    use db_connector::schema::wg_keys::dsl::*;

    let mut conn = get_connection(state)?;
    web_block_unpacked(move || {
        match diesel::delete(wg_keys.filter(charger.eq(cid))).execute(&mut conn) {
            Ok(_) => Ok(()),
            Err(_err) => Err(Error::InternalError),
        }
    })
    .await?;

    Ok(())
}

async fn delete_all_allowed_users(
    cid: String,
    state: &web::Data<AppState>,
) -> Result<(), actix_web::Error> {
    use db_connector::schema::allowed_users::dsl::*;

    let mut conn = get_connection(state)?;
    web_block_unpacked(move || {
        match diesel::delete(allowed_users.filter(charger.eq(cid))).execute(&mut conn) {
            Ok(_) => Ok(()),
            Err(_err) => Err(Error::InternalError),
        }
    })
    .await?;

    Ok(())
}

#[utoipa::path(
    context_path = "/charger",
    request_body = DeleteChargerSchema,
    responses(
        (status = 200, description = "Deletion was successful."),
        (status = 409, description = "The user sending the request is not the owner of the charger.")
    ),
    security(
        ("jwt" = [])
    )
)]
#[delete("/remove")]
pub async fn remove(
    state: web::Data<AppState>,
    uid: crate::models::uuid::Uuid,
    data: web::Json<DeleteChargerSchema>,
) -> Result<impl Responder, actix_web::Error> {
    use db_connector::schema::chargers::dsl::*;

    if !charger_belongs_to_user(&state, uid.clone().into(), data.charger.clone()).await? {
        return Err(Error::Unauthorized.into());
    }

    delete_all_keys(data.charger.clone(), &state).await?;
    delete_all_allowed_users(data.charger.clone(), &state).await?;

    let mut conn = get_connection(&state)?;
    web_block_unpacked(move || {
        match diesel::delete(chargers.filter(id.eq(data.charger.clone()))).execute(&mut conn) {
            Ok(_) => Ok(()),
            Err(_err) => Err(Error::InternalError),
        }
    })
    .await?;

    Ok(HttpResponse::Ok())
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use actix_web::{cookie::Cookie, test, App};
    use db_connector::test_connection_pool;

    use crate::{
        middleware::jwt::JwtMiddleware,
        routes::{
            charger::add::tests::add_test_charger,
            user::tests::{get_test_uuid, TestUser},
        },
        tests::configure,
    };

    pub fn remove_test_keys(mail: &str) {
        use db_connector::schema::wg_keys::dsl::*;

        let uid = get_test_uuid(mail);

        let pool = test_connection_pool();
        let mut conn = pool.get().unwrap();
        diesel::delete(wg_keys.filter(user_id.eq(uid)))
            .execute(&mut conn)
            .unwrap();
    }

    pub fn remove_allowed_test_users(cid: &str) {
        use db_connector::schema::allowed_users::dsl::*;

        let pool = test_connection_pool();
        let mut conn = pool.get().unwrap();
        diesel::delete(allowed_users.filter(charger.eq(cid)))
            .execute(&mut conn)
            .unwrap();
    }

    pub fn remove_test_charger(cid: &str) {
        use db_connector::schema::chargers::dsl::*;

        let pool = test_connection_pool();
        let mut conn = pool.get().unwrap();
        diesel::delete(chargers.filter(id.eq(cid)))
            .execute(&mut conn)
            .unwrap();
    }

    #[actix_web::test]
    async fn test_valid_delete() {
        let app = App::new()
            .configure(configure)
            .wrap(JwtMiddleware)
            .service(remove);
        let app = test::init_service(app).await;

        let mut user = TestUser::new("valid_delete_charger@test.invalid").await;
        let token = user.login().await;
        let charger = "valid_delete_charger";
        add_test_charger(charger, token).await;

        let schema = DeleteChargerSchema {
            charger: charger.to_string(),
        };
        let req = test::TestRequest::delete()
            .uri("/remove")
            .cookie(Cookie::new("access_token", token))
            .set_json(schema)
            .to_request();
        let resp = test::call_service(&app, req).await;
        println!("{:?}", resp);
        assert!(resp.status().is_success());
    }

    #[actix_web::test]
    async fn test_valid_delete_with_allowed_user() {
        let app = App::new()
            .configure(configure)
            .wrap(JwtMiddleware)
            .service(remove);
        let app = test::init_service(app).await;

        let user1 = TestUser::new("valid_delete_charger1@test.invalid").await;
        let mut user2 = TestUser::new("valid_delete_charger2@test.invalid").await;
        let token = user2.login().await.to_owned();
        let charger = "valid_delete_charger1";
        add_test_charger(charger, &token).await;
        user2.allow_user(user1.get_mail(), charger).await;

        let body = DeleteChargerSchema {
            charger: charger.to_string(),
        };
        let req = test::TestRequest::delete()
            .uri("/remove")
            .cookie(Cookie::new("access_token", token))
            .set_json(body)
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_success());
    }

    #[actix_web::test]
    async fn test_unowned_charger_delete() {
        let app = App::new()
            .configure(configure)
            .wrap(JwtMiddleware)
            .service(remove);
        let app = test::init_service(app).await;

        let mut user1 = TestUser::new("unowned_delete_charger1@test.invalid").await;
        let mut user2 = TestUser::new("unowned_delete_charger2@test.invalid").await;
        let charger = "unowned_delete_charger";
        user2.login().await;
        user2.add_charger(charger).await;
        user2.allow_user(user1.get_mail(), charger).await;
        let token = user1.login().await;

        let body = DeleteChargerSchema {
            charger: charger.to_string(),
        };
        let req = test::TestRequest::delete()
            .uri("/remove")
            .set_json(body)
            .cookie(Cookie::new("access_token", token))
            .to_request();
        let resp = test::try_call_service(&app, req).await.unwrap();

        println!("{:?}", resp);
        assert!(resp.status().is_client_error());
        assert!(resp.status().as_u16() == 401);
    }

    #[actix_web::test]
    async fn test_not_allowed_charger_delete() {
        let app = App::new()
            .configure(configure)
            .wrap(JwtMiddleware)
            .service(remove);
        let app = test::init_service(app).await;

        let mut user1 = TestUser::new("not_allowed_delete_charger1@test.invalid").await;
        let mut user2 = TestUser::new("not_allowed_delete_charger2@test.invalid").await;
        let charger = "not_allowed_delete_charger";
        user2.login().await;
        user2.add_charger(charger).await;
        let token = user1.login().await;

        let body = DeleteChargerSchema {
            charger: charger.to_string(),
        };
        let req = test::TestRequest::delete()
            .uri("/remove")
            .set_json(body)
            .cookie(Cookie::new("access_token", token))
            .to_request();
        let resp = test::try_call_service(&app, req).await.unwrap();

        println!("{:?}", resp);
        assert!(resp.status().is_client_error());
        assert!(resp.status().as_u16() == 401);
    }
}
