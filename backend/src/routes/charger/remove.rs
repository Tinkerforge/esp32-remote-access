use actix_web::{delete, web, HttpResponse, Responder};
use diesel::prelude::*;
use serde::{Deserialize, Serialize};

use crate::{error::Error, routes::charger::charger_belongs_to_user, utils::{get_connection, web_block_unpacked}, AppState};

#[derive(Debug, Deserialize, Serialize)]
struct DeleteChargerSchema {
    charger: String
}

async fn delete_all_keys(cid: String, state: &web::Data<AppState>) -> Result<(), actix_web::Error> {
    use crate::schema::wg_keys::dsl::*;

    let mut conn = get_connection(state)?;
    web_block_unpacked(move || {
        match diesel::delete(wg_keys.filter(charger.eq(cid)))
            .execute(&mut conn)
        {
            Ok(_) => Ok(()),
            Err(_err) => {
                Err(Error::InternalError)
            }
        }
    }).await?;

    Ok(())
}

async fn delete_all_allowed_users(cid: String, state: &web::Data<AppState>) -> Result<(), actix_web::Error> {
    use crate::schema::allowed_users::dsl::*;

    let mut conn = get_connection(state)?;
    web_block_unpacked(move || {
        match diesel::delete(allowed_users.filter(charger.eq(cid)))
            .execute(&mut conn)
        {
            Ok(_) => Ok(()),
            Err(_err) => {
                Err(Error::InternalError)
            }
        }
    }).await?;

    Ok(())
}

#[delete("/remove")]
pub async fn remove(
    state: web::Data<AppState>,
    uid: crate::models::uuid::Uuid,
    data: web::Json<DeleteChargerSchema>
) -> Result<impl Responder, actix_web::Error> {
    use crate::schema::chargers::dsl::*;

    charger_belongs_to_user(&state, uid.clone().into(), data.charger.clone()).await?;
    delete_all_keys(data.charger.clone(), &state).await?;
    delete_all_allowed_users(data.charger.clone(), &state).await?;

    let mut conn = get_connection(&state)?;
    web_block_unpacked(move || {
        match diesel::delete(chargers.filter(id.eq(data.charger.clone())))
            .execute(&mut conn)
        {
            Ok(_) => Ok(()),
            Err(_err) => {
                Err(Error::InternalError)
            }
        }
    }).await?;

    Ok(HttpResponse::Ok())
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use actix_web::{cookie::Cookie, test, App};

    use crate::{middleware::jwt::JwtMiddleware, routes::{charger::add::tests::add_test_charger, user::tests::TestUser}, tests::configure};

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
            charger: charger.to_string()
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
}
