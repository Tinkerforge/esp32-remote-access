use actix_web::{post, web, HttpResponse, Responder};
use db_connector::models::allowed_users::AllowedUser;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use diesel::{prelude::*, result::Error::NotFound};

use crate::{error::Error, utils::{get_connection, parse_uuid, web_block_unpacked}, AppState, BridgeState};

#[derive(Serialize, Deserialize, ToSchema)]
pub struct ChargerInfo {
    pub id: String,
    pub name: Option<String>,
    pub configured_port: i32,
    pub connected: bool,
}

#[derive(Serialize, Deserialize, ToSchema)]
pub struct ChargerInfoRequest {
    pub charger: String,
}

#[utoipa::path(
    context_path = "/charger",
    responses(
        (status = 200, description = "Charger information", body = ChargerInfo),
        (status = 400, description = "Charger does not exist"),
    )
)]
#[post("/info")]
pub async fn charger_info(
    state: web::Data<AppState>,
    bridge_state: web::Data<BridgeState>,
    charger: web::Json<ChargerInfoRequest>,
    user: crate::models::uuid::Uuid,
) -> actix_web::Result<impl Responder> {

    let charger_id = parse_uuid(charger.charger.as_str())?;

    let mut conn = get_connection(&state)?;
    let charger: AllowedUser = web_block_unpacked(move || {
        use db_connector::schema::allowed_users::dsl as allowed_users;

        let user: uuid::Uuid = user.into();
        match allowed_users::allowed_users.filter(allowed_users::user_id.eq(user))
            .filter(allowed_users::charger_id.eq(charger_id))
            .select(AllowedUser::as_select())
            .get_result(&mut conn) {
                Ok(charger) => Ok(charger),
                Err(NotFound) => Err(Error::ChargerDoesNotExist),
                Err(_) => Err(Error::InternalError),
            }
    }).await?;

    let mut conn = get_connection(&state)?;
    let port: i32 = web_block_unpacked(move || {
        use db_connector::schema::chargers::dsl::*;

        match chargers.filter(id.eq(charger_id))
            .select(webinterface_port)
            .get_result(&mut conn) {
                Ok(port) => Ok(port),
                Err(_) => Err(Error::InternalError),
            }
    }).await?;

    let map = bridge_state.charger_management_map_with_id.lock().await;
    let connected = map.get(&charger_id).is_some();

    let info = ChargerInfo {
        id: charger_id.to_string(),
        name: charger.name,
        configured_port: port,
        connected,
    };


    Ok(HttpResponse::Ok().json(info))
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use actix_web::{cookie::Cookie, test, App};
    use db_connector::test_connection_pool;
    use diesel::prelude::*;

    use crate::{middleware::jwt::JwtMiddleware, routes::{charger::info::ChargerInfo, user::{me::tests::get_test_user, tests::TestUser}}, tests::configure};

    use super::{charger_info, ChargerInfoRequest};


    #[actix::test]
    async fn test_charger_info() {
        let (mut user, _) = TestUser::random().await;
        let access_token = user.login().await.to_owned();
        let charger = user.add_random_charger().await;
        let _charger = user.add_random_charger().await;

        let app = App::new()
            .configure(configure)
            .wrap(JwtMiddleware)
            .service(charger_info);
        let app = test::init_service(app).await;

        let data = ChargerInfoRequest {
            charger: charger.uuid.clone(),
        };
        let req = test::TestRequest::post()
            .uri("/info")
            .set_json(&data)
            .cookie(Cookie::new("access_token", access_token))
            .to_request();
        let resp = test::call_service(&app, req).await;

        assert_eq!(resp.status(), 200);
        let body: ChargerInfo = test::read_body_json(resp).await;

        let pool = test_connection_pool();
        let mut conn = pool.get().unwrap();
        let user = get_test_user(&user.mail);
        let name: Option<String> = {
            use db_connector::schema::allowed_users::dsl::*;

            let cid = uuid::Uuid::from_str(&charger.uuid).unwrap();

            allowed_users
                .filter(user_id.eq(user.id))
                .filter(charger_id.eq(cid))
                .select(name)
                .get_result(&mut conn)
                .unwrap()
        };
        let port: i32 = {
            use db_connector::schema::chargers::dsl::*;

            chargers
                .filter(id.eq(uuid::Uuid::from_str(&charger.uuid).unwrap()))
                .select(webinterface_port)
                .get_result(&mut conn)
                .unwrap()
        };

        assert_eq!(body.id, charger.uuid);
        assert_eq!(body.name, name);
        assert_eq!(body.configured_port, port)
    }

    #[actix::test]
    async fn test_charger_info_non_existing() {
        let (mut user, _) = TestUser::random().await;
        let access_token = user.login().await.to_owned();
        let _charger = user.add_random_charger().await;
        let non_existent_charger = "00000000-0000-0000-0000-000000000000".to_string();

        let app = App::new()
            .configure(configure)
            .wrap(JwtMiddleware)
            .service(charger_info);
        let app = test::init_service(app).await;

        let data = ChargerInfoRequest {
            charger: non_existent_charger,
        };
        let req = test::TestRequest::post()
            .uri("/info")
            .set_json(&data)
            .cookie(Cookie::new("access_token", access_token))
            .to_request();
        let resp = test::call_service(&app, req).await;

        assert_eq!(resp.status(), 400);
    }
}
