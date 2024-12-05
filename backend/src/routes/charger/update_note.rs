/* esp32-remote-access
 * Copyright (C) 2024 Frederic Henrichs <frederic@tinkerforge.com>
 *
 * This library is free software; you can redistribute it and/or
 * modify it under the terms of the GNU Lesser General Public
 * License as published by the Free Software Foundation; either
 * version 2 of the License, or (at your option) any later version.
 *
 * This library is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU
 * Lesser General Public License for more details.
 *
 * You should have received a copy of the GNU Lesser General Public
 * License along with this library; if not, write to the
 * Free Software Foundation, Inc., 59 Temple Place - Suite 330,
 * Boston, MA 02
 */

use actix_web::{post, web, HttpResponse, Responder};
use diesel::{prelude::*, result::Error::NotFound, ExpressionMethods};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::{error::Error, utils::{get_connection, parse_uuid, web_block_unpacked}, AppState};


 #[derive(ToSchema, Deserialize, Serialize)]
pub struct UpdateNoteSchema {
    pub charger_id: String,
    pub note: String
}

#[utoipa::path(
    context_path = "/charger",
    request_body = UpdateNoteSchema,
    responses(
        (status = 200, description = "Update was successful."),
        (status = 400, description = "Invalid charger-ID")
    ),
    security(
        ("jwt" = [])
    )
)]
#[post("/update_note")]
pub async fn update_note(
    schema: web::Json<UpdateNoteSchema>,
    state: web::Data<AppState>,
    uid: crate::models::uuid::Uuid,
) -> actix_web::Result<impl Responder>
{
    let cid = parse_uuid(&schema.charger_id)?;

    let mut conn = get_connection(&state)?;
    web_block_unpacked(move || {
        use db_connector::schema::allowed_users::dsl::*;

        match diesel::update(allowed_users)
            .filter(charger_id.eq(cid))
            .filter(user_id.eq(uuid::Uuid::from(uid)))
            .set(note.eq(&schema.note))
            .execute(&mut conn)
        {
            Ok(_) => Ok(()),
            Err(NotFound) => Err(Error::ChargerDoesNotExist),
            Err(_err) => Err(Error::InternalError)
        }
    }).await?;

    Ok(HttpResponse::Ok())
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use actix_web::{cookie::Cookie, test, App};
    use db_connector::{models::allowed_users::AllowedUser, test_connection_pool};

    use diesel::prelude::*;
    use crate::{middleware::jwt::JwtMiddleware, routes::user::tests::{get_test_uuid, TestUser}, tests::configure};

    use super::{update_note, UpdateNoteSchema};


    #[actix_web::test]
    async fn test_update_note() {
        let (mut user, _) = TestUser::random().await;
        user.login().await;
        let charger = user.add_random_charger().await;

        let app = App::new()
            .configure(configure)
            .wrap(JwtMiddleware)
            .service(update_note);
        let app = test::init_service(app).await;

        let schema = UpdateNoteSchema {
            charger_id: charger.uuid.clone(),
            note: "Test".to_string(),
        };

        let req = test::TestRequest::post()
            .uri("/update_note")
            .cookie(Cookie::new("access_token", user.access_token.as_ref().unwrap()))
            .set_json(schema)
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 200);

        let pool = test_connection_pool();
        let mut conn = pool.get().unwrap();
        {
            use db_connector::schema::allowed_users::dsl::*;

            let user = get_test_uuid(&user.mail).unwrap();
            let u: AllowedUser = allowed_users
                .filter(charger_id.eq(uuid::Uuid::from_str(&charger.uuid).unwrap()))
                .filter(user_id.eq(user))
                .select(AllowedUser::as_select())
                .get_result(&mut conn)
                .unwrap();
            assert_eq!(u.note.unwrap(), "Test");
        }
    }

    #[actix_web::test]
    async fn test_update_two_chargers() {
        let (mut user, _) = TestUser::random().await;
        user.login().await;
        let charger = user.add_random_charger().await;
        let charger2 = user.add_random_charger().await;

        let app = App::new()
            .configure(configure)
            .wrap(JwtMiddleware)
            .service(update_note);
        let app = test::init_service(app).await;

        let schema = UpdateNoteSchema {
            charger_id: charger.uuid.clone(),
            note: "Test".to_string(),
        };

        let req = test::TestRequest::post()
            .uri("/update_note")
            .cookie(Cookie::new("access_token", user.access_token.as_ref().unwrap()))
            .set_json(schema)
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 200);

        let pool = test_connection_pool();
        let mut conn = pool.get().unwrap();
        {
            use db_connector::schema::allowed_users::dsl::*;

            let user = get_test_uuid(&user.mail).unwrap();
            let u: AllowedUser = allowed_users
                .filter(charger_id.eq(uuid::Uuid::from_str(&charger.uuid).unwrap()))
                .filter(user_id.eq(user))
                .select(AllowedUser::as_select())
                .get_result(&mut conn)
                .unwrap();
            assert_eq!(u.note.unwrap(), "Test");
            let u: AllowedUser = allowed_users
                .filter(charger_id.eq(uuid::Uuid::from_str(&charger2.uuid).unwrap()))
                .filter(user_id.eq(user))
                .select(AllowedUser::as_select())
                .get_result(&mut conn)
                .unwrap();
            assert_eq!(u.note.unwrap(), "");
        }
    }
}
