/* esp32-remote-access
 * Copyright (C) 2025 Frederic Henrichs <frederic@tinkerforge.com>
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
 * Boston, MA 02111-1307, USA.
 */

use actix_web::{post, web, HttpResponse, Responder};
use db_connector::models::device_groupings::DeviceGrouping;
use diesel::prelude::*;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::{
    error::Error,
    utils::{get_connection, web_block_unpacked},
    AppState,
};

#[derive(Serialize, Deserialize, ToSchema)]
pub struct CreateGroupingSchema {
    pub name: String,
    #[serde(default)]
    pub is_default: bool,
}

#[derive(Serialize, Deserialize, ToSchema)]
pub struct CreateGroupingResponse {
    pub id: String,
    pub name: String,
    pub is_default: bool,
}

/// Create a new device grouping
#[utoipa::path(
    context_path = "/grouping",
    request_body = CreateGroupingSchema,
    responses(
        (status = 200, description = "Grouping created successfully", body = CreateGroupingResponse),
        (status = 400, description = "Invalid input"),
        (status = 500, description = "Internal server error")
    ),
    security(
        ("jwt" = [])
    )
)]
#[post("/create")]
pub async fn create_grouping(
    state: web::Data<AppState>,
    payload: web::Json<CreateGroupingSchema>,
    user_id: crate::models::uuid::Uuid,
) -> actix_web::Result<impl Responder> {
    use db_connector::schema::device_groupings::dsl as groupings;

    let name = payload.name.clone();
    let is_default = payload.is_default;
    let user_uuid: uuid::Uuid = user_id.into();
    let mut conn = get_connection(&state)?;

    let grouping = web_block_unpacked(move || {
        if is_default {
            if let Err(_err) = diesel::update(groupings::device_groupings)
                .filter(groupings::user_id.eq(user_uuid))
                .filter(groupings::is_default.eq(true))
                .set(groupings::is_default.eq(false))
                .execute(&mut conn)
            {
                return Err(Error::InternalError);
            }
        }

        let new_grouping = DeviceGrouping {
            id: uuid::Uuid::new_v4(),
            name: name.clone(),
            user_id: user_uuid,
            is_default,
        };

        match diesel::insert_into(groupings::device_groupings)
            .values(&new_grouping)
            .get_result::<DeviceGrouping>(&mut conn)
        {
            Ok(g) => Ok(g),
            Err(_err) => Err(Error::InternalError),
        }
    })
    .await?;

    Ok(HttpResponse::Ok().json(CreateGroupingResponse {
        id: grouping.id.to_string(),
        name: grouping.name,
        is_default: grouping.is_default,
    }))
}

#[cfg(test)]
mod tests {
    use crate::routes::{grouping::test_helpers::*, user::tests::TestUser};

    #[actix_web::test]
    async fn test_create_grouping() {
        let (mut user, _) = TestUser::random().await;
        let token = user.login().await;

        let grouping_name = "Test Grouping";
        let response = create_test_grouping(token, grouping_name).await;

        assert_eq!(response.name, grouping_name);
        assert!(!response.id.is_empty());
        assert!(!response.is_default);

        // Verify grouping exists in database
        let db_grouping = get_grouping_from_db(&response.id);
        assert!(db_grouping.is_some());
        assert_eq!(db_grouping.unwrap().name, grouping_name);

        // Cleanup
        delete_test_grouping_from_db(&response.id);
    }

    #[actix_web::test]
    async fn test_create_grouping_as_default_clears_existing_default() {
        use crate::routes::grouping::create_grouping::{
            CreateGroupingResponse, CreateGroupingSchema,
        };
        use crate::routes::grouping::edit_grouping::EditGroupingSchema;
        use crate::tests::configure as test_configure;
        use actix_web::{cookie::Cookie, test, App};

        let (mut user, _) = TestUser::random().await;
        let token = user.login().await;

        // Create the first grouping and promote it to default.
        let first = create_test_grouping(token, "First").await;
        let first_id = first.id.clone();

        let app = App::new()
            .configure(test_configure)
            .configure(super::super::configure);
        let app = test::init_service(app).await;

        let promote = EditGroupingSchema {
            grouping_id: first_id.clone(),
            name: None,
            is_default: Some(true),
        };
        let cookie = Cookie::new("access_token", token);
        let req = test::TestRequest::put()
            .uri("/grouping/edit")
            .cookie(cookie.clone())
            .set_json(&promote)
            .to_request();
        assert!(test::call_service(&app, req).await.status().is_success());

        // Create the second grouping as default. The first should be cleared.
        let body = CreateGroupingSchema {
            name: "Second".to_string(),
            is_default: true,
        };
        let req = test::TestRequest::post()
            .uri("/grouping/create")
            .cookie(cookie)
            .set_json(&body)
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_success());
        let second: CreateGroupingResponse = test::read_body_json(resp).await;
        assert!(second.is_default);

        assert!(!get_grouping_from_db(&first_id).unwrap().is_default);
        assert!(get_grouping_from_db(&second.id).unwrap().is_default);

        delete_test_grouping_from_db(&first_id);
        delete_test_grouping_from_db(&second.id);
    }
}
