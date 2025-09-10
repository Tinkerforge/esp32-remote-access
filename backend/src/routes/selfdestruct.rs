use actix_web::{delete, web, HttpRequest, HttpResponse, Responder};
use db_connector::models::chargers::Charger;
use diesel::prelude::*;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::{
    error::Error,
    rate_limit::ChargerRateLimiter,
    routes::charger::add::password_matches,
    utils::{
        get_charger_by_uid, get_charger_from_db, get_connection, parse_uuid, web_block_unpacked,
    },
    AppState,
};

#[derive(ToSchema, Serialize, Deserialize)]
pub struct SelfdestructSchema {
    pub id: Option<i32>,
    pub uuid: Option<String>,
    pub password: String,
}

async fn get_charger(
    schema: SelfdestructSchema,
    state: &web::Data<AppState>,
    rate_limiter: &web::Data<ChargerRateLimiter>,
    req: &HttpRequest,
) -> actix_web::Result<Charger> {
    if let Some(uuid) = schema.uuid {
        rate_limiter.check(uuid.clone(), req)?;
        let charger_id = parse_uuid(&uuid)?;
        let charger = get_charger_from_db(charger_id, state).await?;
        if !password_matches(&schema.password, &charger.password)? {
            return Err(Error::ChargerCredentialsWrong.into());
        }
        Ok(charger)
    } else if let Some(uid) = schema.id {
        rate_limiter.check(uid.to_string(), req)?;
        Ok(get_charger_by_uid(uid, Some(schema.password), state).await?)
    } else {
        Err(Error::ChargerCredentialsWrong.into())
    }
}

// Chargers can delete themselves via this route
#[utoipa::path(
    context_path = "/charger",
    request_body = SelfdestructSchema,
    responses(
        (status = 200, description = "Everything worked fine and the charger was deleted")
    )
)]
#[delete("/selfdestruct")]
pub async fn selfdestruct(
    payload: web::Json<SelfdestructSchema>,
    state: web::Data<AppState>,
    rate_limiter: web::Data<ChargerRateLimiter>,
    req: HttpRequest,
) -> actix_web::Result<impl Responder> {
    // funtion does also the rate limiting
    let charger = get_charger(payload.0, &state, &rate_limiter, &req).await?;

    let mut conn = get_connection(&state)?;
    web_block_unpacked(move || {
        use db_connector::schema::allowed_users::dsl as allowed_users;

        match diesel::delete(
            allowed_users::allowed_users.filter(allowed_users::charger_id.eq(charger.id)),
        )
        .execute(&mut conn)
        {
            Ok(_) => Ok(()),
            Err(_err) => Err(Error::InternalError),
        }
    })
    .await?;

    let mut conn = get_connection(&state)?;
    web_block_unpacked(move || {
        use db_connector::schema::wg_keys::dsl as wg_keys;

        match diesel::delete(wg_keys::wg_keys.filter(wg_keys::charger_id.eq(charger.id)))
            .execute(&mut conn)
        {
            Ok(_) => Ok(()),
            Err(_err) => Err(Error::InternalError),
        }
    })
    .await?;

    let mut conn = get_connection(&state)?;
    web_block_unpacked(move || {
        use db_connector::schema::chargers::dsl::*;

        match diesel::delete(chargers.filter(id.eq(charger.id))).execute(&mut conn) {
            Ok(_) => Ok(()),
            Err(_err) => Err(Error::InternalError),
        }
    })
    .await?;

    Ok(HttpResponse::Ok().body(()))
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use actix_web::{
        test::{self, TestRequest},
        App,
    };
    use db_connector::{
        models::{allowed_users::AllowedUser, chargers::Charger, wg_keys::WgKey},
        test_connection_pool,
    };
    use diesel::prelude::*;

    use crate::{routes::user::tests::TestUser, tests::configure};

    use super::{selfdestruct, SelfdestructSchema};

    #[actix_web::test]
    async fn test_selfdestruction() {
        let (mut user, _) = TestUser::random().await;
        user.login().await;
        let charger = user.add_random_charger().await;

        let app = App::new().configure(configure).service(selfdestruct);
        let app = test::init_service(app).await;

        let schema = SelfdestructSchema {
            id: None,
            uuid: Some(charger.uuid.clone()),
            password: charger.password,
        };

        let req = TestRequest::delete()
            .uri("/selfdestruct")
            .append_header(("X-Forwarded-For", "123.123.123.3"))
            .set_json(schema)
            .to_request();

        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 200);
        let pool = test_connection_pool();
        let mut conn = pool.get().unwrap();

        let cid = uuid::Uuid::from_str(&charger.uuid).unwrap();
        let chargers: Vec<Charger> = {
            use db_connector::schema::chargers::dsl as chargers;

            chargers::chargers
                .filter(chargers::id.eq(cid))
                .select(Charger::as_select())
                .load(&mut conn)
                .unwrap()
        };
        assert_eq!(chargers.len(), 0);

        let allowed_users: Vec<AllowedUser> = {
            use db_connector::schema::allowed_users::dsl::*;

            allowed_users
                .filter(charger_id.eq(cid))
                .select(AllowedUser::as_select())
                .load(&mut conn)
                .unwrap()
        };
        assert_eq!(allowed_users.len(), 0);

        let wg_keys: Vec<WgKey> = {
            use db_connector::schema::wg_keys::dsl::*;

            wg_keys
                .filter(charger_id.eq(cid))
                .select(WgKey::as_select())
                .load(&mut conn)
                .unwrap()
        };
        assert_eq!(wg_keys.len(), 0);
    }

    #[actix_web::test]
    async fn test_selfdestruction_depricated() {
        let (mut user, _) = TestUser::random().await;
        user.login().await;
        let charger = user.add_random_charger().await;

        let app = App::new().configure(configure).service(selfdestruct);
        let app = test::init_service(app).await;

        let schema = SelfdestructSchema {
            id: Some(charger.uid),
            uuid: None,
            password: charger.password,
        };

        let req = TestRequest::delete()
            .uri("/selfdestruct")
            .append_header(("X-Forwarded-For", "123.123.123.3"))
            .set_json(schema)
            .to_request();

        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 200);
        let pool = test_connection_pool();
        let mut conn = pool.get().unwrap();

        let cid = uuid::Uuid::from_str(&charger.uuid).unwrap();
        let chargers: Vec<Charger> = {
            use db_connector::schema::chargers::dsl as chargers;

            chargers::chargers
                .filter(chargers::id.eq(cid))
                .select(Charger::as_select())
                .load(&mut conn)
                .unwrap()
        };
        assert_eq!(chargers.len(), 0);

        let allowed_users: Vec<AllowedUser> = {
            use db_connector::schema::allowed_users::dsl::*;

            allowed_users
                .filter(charger_id.eq(cid))
                .select(AllowedUser::as_select())
                .load(&mut conn)
                .unwrap()
        };
        assert_eq!(allowed_users.len(), 0);

        let wg_keys: Vec<WgKey> = {
            use db_connector::schema::wg_keys::dsl::*;

            wg_keys
                .filter(charger_id.eq(cid))
                .select(WgKey::as_select())
                .load(&mut conn)
                .unwrap()
        };
        assert_eq!(wg_keys.len(), 0);
    }
}
