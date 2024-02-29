use actix_web::{get, web, HttpResponse, Responder};
use db_connector::models::{allowed_users::AllowedUser, chargers::Charger};
use diesel::prelude::*;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::{
    error::Error,
    routes::user::get_user,
    utils::{get_connection, web_block_unpacked},
    AppState,
};

#[derive(Serialize, Deserialize, ToSchema)]
pub struct GetChargerSchema {
    id: String,
    name: String,
}

/// Get all chargers that the current user has access to.
#[utoipa::path(
    context_path = "/charger",
    responses(
        (status = 200, description = "Success", body = [GetChargerSchema]),
        (status = 400, description = "Somehow got a valid jwt but the user does not exist.")
    ),
    security(
        ("jwt" = [])
    )
)]
#[get("/get_chargers")]
pub async fn get_chargers(
    state: web::Data<AppState>,
    uid: crate::models::uuid::Uuid,
) -> Result<impl Responder, actix_web::Error> {
    use db_connector::schema::allowed_users::dsl as allowed_users;
    use db_connector::schema::chargers::dsl as chargers;

    let user = get_user(&state, uid.into()).await?;

    let mut conn = get_connection(&state)?;
    let charger: Vec<Charger> = web_block_unpacked(move || {
        let charger_ids = AllowedUser::belonging_to(&user).select(allowed_users::charger_id);
        match chargers::chargers
            .filter(chargers::id.eq_any(charger_ids))
            .select(Charger::as_select())
            .load(&mut conn)
        {
            Ok(v) => Ok(v),
            Err(_err) => Err(Error::InternalError),
        }
    })
    .await?;

    let charger = charger
        .into_iter()
        .map(|c| GetChargerSchema {
            id: c.id,
            name: c.name,
        })
        .collect::<Vec<GetChargerSchema>>();

    Ok(HttpResponse::Ok().json(charger))
}

#[cfg(test)]
mod tests {
    use actix_web::{cookie::Cookie, test, App};

    use super::*;
    use crate::{middleware::jwt::JwtMiddleware, routes::user::tests::TestUser, tests::configure};

    /// Test if only the chargers the user has access to will be returned.
    #[actix_web::test]
    async fn test_get_chargers() {
        let mut owned_chargers: Vec<String> = Vec::new();
        let mut accessable_chargers: Vec<String> = Vec::new();
        let (mut user1, mail1) = TestUser::random().await;
        let (mut user2, _) = TestUser::random().await;
        user1.login().await;
        user2.login().await;
        for _ in 0..5 {
            let uuid1 = uuid::Uuid::new_v4().to_string();
            let uuid2 = uuid::Uuid::new_v4().to_string();
            user1.add_charger(&uuid1).await;
            user2.add_charger(&uuid2).await;
            user2.allow_user(&mail1, &uuid2).await;
            owned_chargers.push(uuid1);
            accessable_chargers.push(uuid2);
        }
        for _ in 0..5 {
            let uuid = uuid::Uuid::new_v4().to_string();
            user2.add_charger(&uuid).await;
        }

        let app = App::new()
            .configure(configure)
            .wrap(JwtMiddleware)
            .service(get_chargers);
        let app = test::init_service(app).await;

        let req = test::TestRequest::get()
            .uri("/get_chargers")
            .cookie(Cookie::new("access_token", user1.get_token()))
            .to_request();
        let resp: Vec<GetChargerSchema> = test::call_and_read_body_json(&app, req).await;
        assert_eq!(resp.len(), 10);
    }

    #[actix_web::test]
    async fn test_get_not_existing_chargers() {
        let (mut user1, _) = TestUser::random().await;
        user1.login().await;

        let app = App::new()
            .configure(configure)
            .wrap(JwtMiddleware)
            .service(get_chargers);
        let app = test::init_service(app).await;

        let req = test::TestRequest::get()
            .uri("/get_chargers")
            .cookie(Cookie::new("access_token", user1.get_token()))
            .to_request();
        let resp: Vec<GetChargerSchema> = test::call_and_read_body_json(&app, req).await;
        assert_eq!(resp.len(), 0);
    }
}