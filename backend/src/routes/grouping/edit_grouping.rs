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
    pub name: Option<String>,
    pub is_default: Option<bool>,
}

#[derive(Serialize, Deserialize, ToSchema)]
pub struct EditGroupingResponse {
    pub id: String,
    pub name: String,
    pub is_default: bool,
}

#[utoipa::path(
    context_path = "/grouping",
    request_body = EditGroupingSchema,
    responses(
        (status = 200, description = "Grouping updated successfully", body = EditGroupingResponse),
        (status = 400, description = "Invalid grouping ID, grouping not found, or no fields to update"),
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
    let new_is_default = payload.is_default;
    let user_uuid: uuid::Uuid = user_id.into();
    let mut conn = get_connection(&state)?;

    if new_name.is_none() && new_is_default.is_none() {
        return Err(Error::InvalidPayload.into());
    }

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

        let promoting_to_default = new_is_default == Some(true) && !grouping.is_default;
        if promoting_to_default {
            if let Err(_err) = diesel::update(groupings::device_groupings)
                .filter(groupings::user_id.eq(user_uuid))
                .filter(groupings::is_default.eq(true))
                .filter(groupings::id.ne(grouping_uuid))
                .set(groupings::is_default.eq(false))
                .execute(&mut conn)
            {
                return Err(Error::InternalError);
            }
        }

        // Apply the update. Exactly one of the two fields is guaranteed to
        // be present thanks to the early return above, but handling both
        // keeps the path open for callers that want to update both at once.
        let result = match (new_name.as_ref(), new_is_default) {
            (Some(name), Some(is_default)) => {
                diesel::update(groupings::device_groupings.find(grouping_uuid))
                    .set((
                        groupings::name.eq(name),
                        groupings::is_default.eq(is_default),
                    ))
                    .get_result::<DeviceGrouping>(&mut conn)
            }
            (Some(name), None) => diesel::update(groupings::device_groupings.find(grouping_uuid))
                .set(groupings::name.eq(name))
                .get_result::<DeviceGrouping>(&mut conn),
            (None, Some(is_default)) => {
                diesel::update(groupings::device_groupings.find(grouping_uuid))
                    .set(groupings::is_default.eq(is_default))
                    .get_result::<DeviceGrouping>(&mut conn)
            }
            (None, None) => return Err(Error::InvalidPayload),
        };

        match result {
            Ok(g) => Ok(g),
            Err(_err) => Err(Error::InternalError),
        }
    })
    .await?;

    Ok(HttpResponse::Ok().json(EditGroupingResponse {
        id: updated_grouping.id.to_string(),
        name: updated_grouping.name,
        is_default: updated_grouping.is_default,
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
            name: Some(new_name.to_string()),
            is_default: None,
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
            name: Some("Hacked Name".to_string()),
            is_default: None,
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
            name: Some("New Name".to_string()),
            is_default: None,
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

    #[actix_web::test]
    async fn test_edit_grouping_no_fields() {
        let (mut user, _) = TestUser::random().await;
        let token = user.login().await;

        let grouping = create_test_grouping(token, "Some Group").await;

        let app = App::new().configure(test_configure).configure(configure);
        let app = test::init_service(app).await;

        let body = EditGroupingSchema {
            grouping_id: grouping.id.clone(),
            name: None,
            is_default: None,
        };

        let cookie = Cookie::new("access_token", token);
        let req = test::TestRequest::put()
            .uri("/grouping/edit")
            .cookie(cookie)
            .set_json(&body)
            .to_request();

        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status().as_u16(), 400); // InvalidPayload

        delete_test_grouping_from_db(&grouping.id);
    }

    #[actix_web::test]
    async fn test_edit_grouping_promote_to_default() {
        let (mut user, _) = TestUser::random().await;
        let token = user.login().await;

        let grouping = create_test_grouping(token, "Group").await;
        assert!(!grouping.is_default);

        let app = App::new().configure(test_configure).configure(configure);
        let app = test::init_service(app).await;

        let body = EditGroupingSchema {
            grouping_id: grouping.id.clone(),
            name: None,
            is_default: Some(true),
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
        assert!(response.is_default);

        let db_grouping = get_grouping_from_db(&grouping.id).unwrap();
        assert!(db_grouping.is_default);

        delete_test_grouping_from_db(&grouping.id);
    }

    #[actix_web::test]
    async fn test_edit_grouping_promote_clears_previous_default() {
        let (mut user, _) = TestUser::random().await;
        let token = user.login().await;

        // Create the first grouping and promote it to default.
        let first = create_test_grouping(token, "First").await;
        let app = App::new().configure(test_configure).configure(configure);
        let app = test::init_service(app).await;

        let promote_first = EditGroupingSchema {
            grouping_id: first.id.clone(),
            name: None,
            is_default: Some(true),
        };
        let cookie = Cookie::new("access_token", token);
        let req = test::TestRequest::put()
            .uri("/grouping/edit")
            .cookie(cookie.clone())
            .set_json(&promote_first)
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_success());

        // Create a second grouping.
        let second = create_test_grouping(token, "Second").await;

        // Promote the second one to default. The first should be cleared.
        let promote_second = EditGroupingSchema {
            grouping_id: second.id.clone(),
            name: None,
            is_default: Some(true),
        };
        let req = test::TestRequest::put()
            .uri("/grouping/edit")
            .cookie(cookie)
            .set_json(&promote_second)
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_success());

        assert!(!get_grouping_from_db(&first.id).unwrap().is_default);
        assert!(get_grouping_from_db(&second.id).unwrap().is_default);

        delete_test_grouping_from_db(&first.id);
        delete_test_grouping_from_db(&second.id);
    }

    #[actix_web::test]
    async fn test_edit_grouping_clear_default() {
        let (mut user, _) = TestUser::random().await;
        let token = user.login().await;

        let grouping = create_test_grouping(token, "Group").await;

        let app = App::new().configure(test_configure).configure(configure);
        let app = test::init_service(app).await;

        // Promote to default first.
        let promote = EditGroupingSchema {
            grouping_id: grouping.id.clone(),
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
        assert!(get_grouping_from_db(&grouping.id).unwrap().is_default);

        // Clear the default flag.
        let clear = EditGroupingSchema {
            grouping_id: grouping.id.clone(),
            name: None,
            is_default: Some(false),
        };
        let req = test::TestRequest::put()
            .uri("/grouping/edit")
            .cookie(cookie)
            .set_json(&clear)
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_success());

        let response: EditGroupingResponse = test::read_body_json(resp).await;
        assert!(!response.is_default);
        assert!(!get_grouping_from_db(&grouping.id).unwrap().is_default);

        delete_test_grouping_from_db(&grouping.id);
    }
}
