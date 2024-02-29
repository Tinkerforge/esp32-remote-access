use actix_web::{get, web, HttpResponse, Responder};
use db_connector::models::wg_keys::WgKey;
use diesel::{prelude::*, result::Error::NotFound};
use ipnetwork::IpNetwork;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::{
    error::Error,
    routes::user::get_user,
    utils::{get_connection, web_block_unpacked},
    AppState,
};

#[derive(Serialize, Deserialize, ToSchema)]
pub struct GetWgKeysSchema {
    id: String,
    charger_id: String,
    charger_pub: String,
    charger_address: IpNetwork,
    web_private: String,
    web_address: IpNetwork,
}

#[derive(Serialize, Deserialize)]
pub struct GetWgKeysQuery {
    cid: String,
}

#[utoipa::path(
    context_path = "/charger",
    responses(
        (status = 200, body = GetWgKeysSchema),
        (status = 400, description = "Somehow got a valid jwt but the user does not exist.")
    ),
    security(
        ("jwt" = [])
    )
)]
#[get("/get_key")]
pub async fn get_key(
    state: web::Data<AppState>,
    uid: crate::models::uuid::Uuid,
    web_query: web::Query<GetWgKeysQuery>,
) -> actix_web::Result<impl Responder> {
    use db_connector::schema::wg_keys::dsl::*;

    let user = get_user(&state, uid.into()).await?;

    let mut conn = get_connection(&state)?;
    let key: Option<WgKey> = web_block_unpacked(move || {
        match WgKey::belonging_to(&user)
            .filter(charger_id.eq(&web_query.cid))
            .filter(in_use.eq(false))
            .select(WgKey::as_select())
            .get_result(&mut conn)
        {
            Ok(v) => Ok(Some(v)),
            Err(NotFound) => Ok(None),
            Err(_err) => Err(Error::InternalError),
        }
    })
    .await?;

    if let Some(key) = key {
        let key = GetWgKeysSchema {
            id: key.id.to_string(),
            charger_id: key.charger_id,
            charger_pub: key.charger_pub,
            charger_address: key.charger_address,
            web_private: key.web_private,
            web_address: key.web_address,
        };
        Ok(HttpResponse::Ok().json(key))
    } else {
        Err(Error::AllKeysInUse.into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::{cookie::Cookie, test, App};
    use db_connector::test_connection_pool;

    use crate::{middleware::jwt::JwtMiddleware, routes::user::tests::TestUser, tests::configure};

    #[actix_web::test]
    async fn test_get_key() {
        let (mut user, _) = TestUser::random().await;
        user.login().await;

        let charger = uuid::Uuid::new_v4().to_string();
        user.add_charger(&charger).await;

        let app = App::new()
            .configure(configure)
            .wrap(JwtMiddleware)
            .service(get_key);
        let app = test::init_service(app).await;

        let req = test::TestRequest::get()
            .uri(&format!("/get_key?cid={}", charger.clone()))
            .cookie(Cookie::new("access_token", user.get_token()))
            .to_request();

        let resp: GetWgKeysSchema = test::call_and_read_body_json(&app, req).await;
        assert_eq!(resp.charger_id, charger);
    }

    #[actix_web::test]
    async fn test_get_key_none_left() {
        use db_connector::schema::wg_keys::dsl::*;

        let (mut user, _) = TestUser::random().await;
        user.login().await;

        let charger = uuid::Uuid::new_v4().to_string();
        user.add_charger(&charger).await;

        let pool = test_connection_pool();
        let mut conn = pool.get().unwrap();

        diesel::update(wg_keys)
            .filter(charger_id.eq(&charger))
            .set(in_use.eq(true))
            .execute(&mut conn)
            .unwrap();

        let app = App::new()
            .configure(configure)
            .wrap(JwtMiddleware)
            .service(get_key);
        let app = test::init_service(app).await;

        let req = test::TestRequest::get()
            .uri(&format!("/get_key?cid={}", charger.clone()))
            .cookie(Cookie::new("access_token", user.get_token()))
            .to_request();

        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_client_error());
        assert_eq!(resp.status().as_u16(), 404);
    }
}