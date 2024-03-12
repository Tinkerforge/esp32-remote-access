use std::time::{SystemTime, UNIX_EPOCH};

use actix_web::{put, web, HttpRequest, HttpResponse, Responder};
use db_connector::models::allowed_users::AllowedUser;
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
pub struct ManagementSchema {
    id: String,
}

#[derive(Serialize, Deserialize, ToSchema)]
pub struct ManagementResponseSchema {
    time: u64,
}

/// Route for the charger to be identifiable via the ip.
#[utoipa::path(
    request_body = ManagementSchema,
    responses(
        (status = 200, description = "Identification was successful", body = ManagementResponseSchema),
        (status = 400, description = "Got no valid ip address for the charger"),
        (status = 401, description = "The logged in user is not the owner of the charger")
    )
)]
#[put("/management")]
pub async fn management(
    req: HttpRequest,
    state: web::Data<AppState>,
    uid: crate::models::uuid::Uuid,
    data: web::Json<ManagementSchema>,
) -> actix_web::Result<impl Responder> {
    use db_connector::schema::allowed_users::dsl as allowed_users;
    use db_connector::schema::chargers::dsl as chargers;

    let user = get_user(&state, uid.into()).await?;

    let info = req.connection_info();
    let ip = info.realip_remote_addr();

    if ip.is_none() {
        return Err(Error::NoValidIp.into());
    }

    let ip = ip.unwrap();

    let mut conn = get_connection(&state)?;
    let allowed_user: AllowedUser = web_block_unpacked(move || {
        match AllowedUser::belonging_to(&user)
            .filter(allowed_users::charger_id.eq(&data.id))
            .select(AllowedUser::as_select())
            .get_result(&mut conn)
        {
            Ok(user) => Ok(user),
            Err(NotFound) => Err(Error::UserIsNotOwner),
            Err(_err) => Err(Error::InternalError),
        }
    })
    .await?;

    if !allowed_user.is_owner {
        return Err(Error::UserIsNotOwner.into());
    }

    let mut conn = get_connection(&state)?;
    let ip: IpNetwork = match ip.parse() {
        Ok(ip) => ip,
        Err(_err) => return Err(Error::InternalError.into()),
    };

    web_block_unpacked(move || {
        match diesel::update(chargers::chargers)
            .filter(chargers::id.eq(allowed_user.charger_id))
            .set(chargers::last_ip.eq(Some(ip)))
            .execute(&mut conn)
        {
            Ok(_) => Ok(()),
            Err(_err) => Err(Error::InternalError),
        }
    })
    .await?;

    let time = match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(time) => time,
        Err(err) => {
            log::error!("Error while getting current time: {}", err);
            return Err(Error::InternalError.into());
        }
    };

    let time = time.as_secs();
    let resp = ManagementResponseSchema { time };

    Ok(HttpResponse::Ok().json(resp))
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::{cookie::Cookie, test, App};

    use crate::{middleware::jwt::JwtMiddleware, routes::user::tests::TestUser, tests::configure};

    #[actix_web::test]
    async fn test_management() {
        let (mut user, _) = TestUser::random().await;
        let token = user.login().await.to_owned();
        let charger = user.add_random_charger().await;

        let app = App::new()
            .configure(configure)
            .wrap(JwtMiddleware)
            .service(management);
        let app = test::init_service(app).await;

        let body = ManagementSchema { id: charger };
        let req = test::TestRequest::put()
            .uri("/management")
            .cookie(Cookie::new("access_token", token))
            .append_header(("X-Forwarded-For", "123.123.123.3"))
            .cookie(Cookie::new("X-Forwarded-For", "123.123.123.3"))
            .set_json(body)
            .to_request();
        let resp = test::call_service(&app, req).await;

        println!("{:?}", resp);
        println!("{:?}", resp.response().body());
        assert!(resp.status().is_success());
    }

    #[actix_web::test]
    async fn test_unowned_charger() {
        let (mut user1, mail1) = TestUser::random().await;
        let token = user1.login().await.to_owned();
        let (mut user2, _) = TestUser::random().await;
        user2.login().await;
        let charger = user2.add_random_charger().await;
        user2.allow_user(&mail1, &charger).await;

        let app = App::new()
            .configure(configure)
            .wrap(JwtMiddleware)
            .service(management);
        let app = test::init_service(app).await;

        let body = ManagementSchema { id: charger };
        let req = test::TestRequest::put()
            .uri("/management")
            .cookie(Cookie::new("access_token", token))
            .append_header(("X-Forwarded-For", "123.123.123.3"))
            .set_json(body)
            .to_request();
        let resp = test::call_service(&app, req).await;

        println!("{:?}", resp);
        println!("{:?}", resp.response().body());
        assert!(resp.status().is_client_error());
        assert_eq!(resp.status().as_u16(), 401);
    }
}
