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

use actix_web::{put, web, HttpResponse, Responder};
use db_connector::models::device_groupings::DeviceGrouping;
use diesel::prelude::*;
use diesel::result::Error::NotFound;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::{
    error::Error,
    utils::{get_connection, parse_uuid, web_block_unpacked},
    AppState,
};

#[derive(Serialize, Deserialize, ToSchema)]
pub struct EditGroupingSchema {
    pub grouping_id: String,
    pub name: String,
}

#[derive(Serialize, Deserialize, ToSchema)]
pub struct EditGroupingResponse {
    pub id: String,
    pub name: String,
}

/// Edit the name of a device grouping
#[utoipa::path(
    context_path = "/grouping",
    request_body = EditGroupingSchema,
    responses(
        (status = 200, description = "Grouping name updated successfully", body = EditGroupingResponse),
        (status = 400, description = "Invalid grouping ID or grouping not found"),
        (status = 401, description = "Unauthorized - user does not own this grouping"),
        (status = 500, description = "Internal server error")
    ),
    security(
        ("jwt" = [])
    )
)]
#[put("/edit")]
pub async fn edit_grouping(
    state: web::Data<AppState>,
    payload: web::Json<EditGroupingSchema>,
    user_id: crate::models::uuid::Uuid,
) -> actix_web::Result<impl Responder> {
    use db_connector::schema::device_groupings::dsl as groupings;

    let grouping_uuid = parse_uuid(&payload.grouping_id)?;
    let new_name = payload.name.clone();
    let user_uuid: uuid::Uuid = user_id.into();
    let mut conn = get_connection(&state)?;

    let updated_grouping = web_block_unpacked(move || {
        // First verify the grouping exists and belongs to the user
        let grouping: DeviceGrouping = match groupings::device_groupings
            .find(grouping_uuid)
            .select(DeviceGrouping::as_select())
            .get_result(&mut conn)
        {
            Ok(g) => g,
            Err(NotFound) => return Err(Error::ChargerDoesNotExist),
            Err(_err) => return Err(Error::InternalError),
        };

        // Verify ownership
        if grouping.user_id != user_uuid {
            return Err(Error::Unauthorized);
        }

        // Update the grouping name
        match diesel::update(groupings::device_groupings.find(grouping_uuid))
            .set(groupings::name.eq(new_name))
            .get_result::<DeviceGrouping>(&mut conn)
        {
            Ok(g) => Ok(g),
            Err(_err) => Err(Error::InternalError),
        }
    })
    .await?;

    Ok(HttpResponse::Ok().json(EditGroupingResponse {
        id: updated_grouping.id.to_string(),
        name: updated_grouping.name,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::{cookie::Cookie, test, App};

    use crate::{
        routes::{
            grouping::{configure, test_helpers::*},
            user::tests::TestUser,
        },
        tests::configure as test_configure,
    };

    #[actix_web::test]
    async fn test_edit_grouping() {
        let (mut user, _) = TestUser::random().await;
        let token = user.login().await;

        let grouping = create_test_grouping(token, "Original Name").await;

        let app = App::new().configure(test_configure).configure(configure);
        let app = test::init_service(app).await;

        let new_name = "Updated Name";
        let body = EditGroupingSchema {
            grouping_id: grouping.id.clone(),
            name: new_name.to_string(),
        };

        let cookie = Cookie::new("access_token", token);
        let req = test::TestRequest::put()
            .uri("/grouping/edit")
            .cookie(cookie)
            .set_json(&body)
            .to_request();

        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_success());

        let response: EditGroupingResponse = test::read_body_json(resp).await;
        assert_eq!(response.name, new_name);
        assert_eq!(response.id, grouping.id);

        // Verify the name was updated in the database
        let db_grouping = get_grouping_from_db(&grouping.id);
        assert!(db_grouping.is_some());
        assert_eq!(db_grouping.unwrap().name, new_name);

        // Cleanup
        delete_test_grouping_from_db(&grouping.id);
    }

    #[actix_web::test]
    async fn test_edit_grouping_unauthorized() {
        let (mut user1, _) = TestUser::random().await;
        let (mut user2, _) = TestUser::random().await;
        let token1 = user1.login().await;
        let token2 = user2.login().await;

        // User 1 creates a grouping
        let grouping = create_test_grouping(token1, "User1 Grouping").await;
        let original_name = grouping.name.clone();

        let app = App::new().configure(test_configure).configure(configure);
        let app = test::init_service(app).await;

        // User 2 tries to edit it
        let body = EditGroupingSchema {
            grouping_id: grouping.id.clone(),
            name: "Hacked Name".to_string(),
        };

        let cookie = Cookie::new("access_token", token2);
        let req = test::TestRequest::put()
            .uri("/grouping/edit")
            .cookie(cookie)
            .set_json(&body)
            .to_request();

        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status().as_u16(), 401); // Unauthorized

        // Verify the name was not changed
        let db_grouping = get_grouping_from_db(&grouping.id);
        assert!(db_grouping.is_some());
        assert_eq!(db_grouping.unwrap().name, original_name);

        // Cleanup
        delete_test_grouping_from_db(&grouping.id);
    }

    #[actix_web::test]
    async fn test_edit_grouping_not_found() {
        let (mut user, _) = TestUser::random().await;
        let token = user.login().await;

        let app = App::new().configure(test_configure).configure(configure);
        let app = test::init_service(app).await;

        let body = EditGroupingSchema {
            grouping_id: uuid::Uuid::new_v4().to_string(),
            name: "New Name".to_string(),
        };

        let cookie = Cookie::new("access_token", token);
        let req = test::TestRequest::put()
            .uri("/grouping/edit")
            .cookie(cookie)
            .set_json(&body)
            .to_request();

        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status().as_u16(), 400); // Bad request (ChargerDoesNotExist returns 400)
    }
}
