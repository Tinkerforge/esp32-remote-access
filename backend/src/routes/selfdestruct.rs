use actix_web::{delete, web, HttpResponse, Responder};
use diesel::prelude::*;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::{error::Error, routes::charger::add::{get_charger_from_db, password_matches}, utils::{get_connection, web_block_unpacked}, AppState};

#[derive(ToSchema, Serialize, Deserialize)]
pub struct SelfdestructSchema {
    pub id: i32,
    pub password: String
}

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
) -> actix_web::Result<impl Responder> {
    let charger = get_charger_from_db(payload.id, &state).await?;
    if !password_matches(payload.password.clone(), charger.password)? {
        return Err(Error::Unauthorized.into())
    }

    let charger_id = payload.id;
    let mut conn = get_connection(&state)?;
    web_block_unpacked(move || {
        use db_connector::schema::allowed_users::dsl as allowed_users;

        match diesel::delete(allowed_users::allowed_users.filter(allowed_users::charger_id.eq(charger_id))).execute(&mut conn) {
            Ok(_) => Ok(()),
            Err(_err) => {
                Err(Error::InternalError)
            }
        }
    }).await?;

    let mut conn = get_connection(&state)?;
    web_block_unpacked(move || {
        use db_connector::schema::wg_keys::dsl as wg_keys;

        match diesel::delete(wg_keys::wg_keys.filter(wg_keys::charger_id.eq(charger_id))).execute(&mut conn) {
            Ok(_) => Ok(()),
            Err(_err) => {
                Err(Error::InternalError)
            }
        }
    }).await?;

    let mut conn = get_connection(&state)?;
    web_block_unpacked(move || {
        use db_connector::schema::chargers::dsl::*;

        match diesel::delete(chargers.filter(id.eq(charger_id))).execute(&mut conn) {
            Ok(_) => Ok(()),
            Err(_err) => {
                Err(Error::InternalError)
            }
        }
    }).await?;

    Ok(HttpResponse::Ok().body(()))
}

#[cfg(test)]
mod tests {
    use actix_web::{test::{self, TestRequest}, App};
    use diesel::prelude::*;
    use db_connector::{models::{allowed_users::AllowedUser, chargers::Charger, wg_keys::WgKey}, test_connection_pool};

    use crate::{routes::user::tests::TestUser, tests::configure};

    use super::{selfdestruct, SelfdestructSchema};


    #[actix_web::test]
    async fn test_selfdestruction() {
        let (mut user, _) = TestUser::random().await;
        user.login().await;
        let (charger, charger_pass) = user.add_random_charger().await;

        let app = App::new().configure(configure).service(selfdestruct);
        let app = test::init_service(app).await;

        let schema = SelfdestructSchema {
            id: charger,
            password: charger_pass
        };

        let req = TestRequest::delete()
            .uri("/selfdestruct")
            .set_json(schema)
            .to_request();

        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 200);
        let pool = test_connection_pool();
        let mut conn = pool.get().unwrap();

        let chargers: Vec<Charger> = {
            use db_connector::schema::chargers::dsl as chargers;

            chargers::chargers.filter(chargers::id.eq(charger)).select(Charger::as_select()).load(&mut conn).unwrap()
        };
        assert_eq!(chargers.len(), 0);

        let allowed_users: Vec<AllowedUser> = {
            use db_connector::schema::allowed_users::dsl::*;

            allowed_users.filter(charger_id.eq(charger)).select(AllowedUser::as_select()).load(&mut conn).unwrap()
        };
        assert_eq!(allowed_users.len(), 0);

        let wg_keys: Vec<WgKey> = {
            use db_connector::schema::wg_keys::dsl::*;

            wg_keys.filter(charger_id.eq(charger)).select(WgKey::as_select()).load(&mut conn).unwrap()
        };
        assert_eq!(wg_keys.len(), 0);
    }
}
