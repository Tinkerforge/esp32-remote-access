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
use db_connector::models::{
    device_grouping_members::DeviceGroupingMember, device_groupings::DeviceGrouping,
};
use diesel::prelude::*;
use diesel::result::Error::NotFound;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::{
    error::Error,
    routes::charger::user_is_allowed,
    utils::{get_connection, parse_uuid, web_block_unpacked},
    AppState,
};

#[derive(Serialize, Deserialize, ToSchema)]
pub struct AddDeviceToGroupingSchema {
    pub grouping_id: String,
    pub device_id: String,
}

#[derive(Serialize, Deserialize, ToSchema)]
pub struct AddDeviceToGroupingResponse {
    pub id: String,
    pub grouping_id: String,
    pub device_id: String,
}

/// Add a device to a grouping
#[utoipa::path(
    context_path = "/grouping",
    request_body = AddDeviceToGroupingSchema,
    responses(
        (status = 200, description = "Device added to grouping successfully", body = AddDeviceToGroupingResponse),
        (status = 400, description = "Invalid input"),
        (status = 401, description = "Unauthorized - user does not own grouping or is not allowed to access charger"),
        (status = 404, description = "Grouping or charger not found"),
        (status = 409, description = "Device already in grouping"),
        (status = 500, description = "Internal server error")
    ),
    security(
        ("jwt" = [])
    )
)]
#[post("/add_device")]
pub async fn add_device_to_grouping(
    state: web::Data<AppState>,
    payload: web::Json<AddDeviceToGroupingSchema>,
    user_id: crate::models::uuid::Uuid,
) -> actix_web::Result<impl Responder> {
    use db_connector::schema::device_groupings::dsl as groupings;

    let grouping_uuid = parse_uuid(&payload.grouping_id)?;
    let charger_uuid = parse_uuid(&payload.device_id)?;
    let user_uuid: uuid::Uuid = user_id.clone().into();

    // Verify user owns the grouping
    let mut conn = get_connection(&state)?;
    let grouping = web_block_unpacked(move || {
        let grouping: DeviceGrouping = match groupings::device_groupings
            .find(grouping_uuid)
            .select(DeviceGrouping::as_select())
            .get_result(&mut conn)
        {
            Ok(g) => g,
            Err(NotFound) => return Err(Error::ChargerDoesNotExist),
            Err(_err) => return Err(Error::InternalError),
        };

        // Verify ownership of grouping
        if grouping.user_id != user_uuid {
            return Err(Error::Unauthorized);
        }

        Ok(grouping)
    })
    .await?;

    // Verify user has access to the charger
    let is_allowed = user_is_allowed(&state, user_uuid, charger_uuid).await?;
    if !is_allowed {
        return Err(Error::Unauthorized.into());
    }

    // Add the device to the grouping
    let mut conn = get_connection(&state)?;
    let member = web_block_unpacked(move || {
        use db_connector::schema::device_grouping_members::dsl as members;

        let new_member = DeviceGroupingMember {
            id: uuid::Uuid::new_v4(),
            grouping_id: grouping.id,
            charger_id: charger_uuid,
            added_at: chrono::Utc::now().naive_utc(),
        };

        match diesel::insert_into(members::device_grouping_members)
            .values(&new_member)
            .get_result::<DeviceGroupingMember>(&mut conn)
        {
            Ok(m) => Ok(m),
            Err(diesel::result::Error::DatabaseError(
                diesel::result::DatabaseErrorKind::UniqueViolation,
                _,
            )) => Err(Error::ChargerAlreadyExists),
            Err(_err) => Err(Error::InternalError),
        }
    })
    .await?;

    Ok(HttpResponse::Ok().json(AddDeviceToGroupingResponse {
        id: member.id.to_string(),
        grouping_id: member.grouping_id.to_string(),
        device_id: member.charger_id.to_string(),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::{cookie::Cookie, test, App};
    use rand::TryRngCore;
    use rand_core::OsRng;

    use crate::{
        routes::{
            grouping::{
                configure, get_groupings::GetGroupingsResponse, test_helpers::*,
            },
            user::tests::TestUser,
        },
        tests::configure as test_configure,
    };

    #[actix_web::test]
    async fn test_add_device_to_grouping() {
        let (mut user, _) = TestUser::random().await;
        let token = user.login().await.to_string();

        // Create a charger
        let charger_id = OsRng.try_next_u32().unwrap() as i32;
        let charger = user.add_charger(charger_id).await;

        // Create a grouping
        let grouping = create_test_grouping(&token, "Test Group").await;

        let app = App::new()
            .configure(test_configure)
            .configure(configure);
        let app = test::init_service(app).await;

        let body = AddDeviceToGroupingSchema {
            grouping_id: grouping.id.clone(),
            device_id: charger.uuid.clone(),
        };

        let cookie = Cookie::new("access_token", token);
        let req = test::TestRequest::post()
            .uri("/grouping/add_device")
            .cookie(cookie)
            .set_json(&body)
            .to_request();

        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_success());

        let response: AddDeviceToGroupingResponse = test::read_body_json(resp).await;
        assert_eq!(response.grouping_id, grouping.id);
        assert_eq!(response.device_id, charger.uuid);

        // Verify member exists in database
        let member_count = count_grouping_members(&grouping.id);
        assert_eq!(member_count, 1);

        // Cleanup
        delete_test_grouping_from_db(&grouping.id);
    }

    #[actix_web::test]
    async fn test_add_device_to_grouping_duplicate() {
        let (mut user, _) = TestUser::random().await;
        let token = user.login().await.to_string();

        let charger_id = OsRng.try_next_u32().unwrap() as i32;
        let charger = user.add_charger(charger_id).await;
        let grouping = create_test_grouping(&token, "Test Group").await;

        let app = App::new()
            .configure(test_configure)
            .configure(configure);
        let app = test::init_service(app).await;

        let body = AddDeviceToGroupingSchema {
            grouping_id: grouping.id.clone(),
            device_id: charger.uuid.clone(),
        };

        // Add device first time
        let cookie = Cookie::new("access_token", token);
        let req = test::TestRequest::post()
            .uri("/grouping/add_device")
            .cookie(cookie.clone())
            .set_json(&body)
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_success());

        // Try to add same device again
        let req = test::TestRequest::post()
            .uri("/grouping/add_device")
            .cookie(cookie)
            .set_json(&body)
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status().as_u16(), 409); // Conflict

        // Cleanup
        delete_test_grouping_from_db(&grouping.id);
    }

    #[actix_web::test]
    async fn test_add_device_to_grouping_unauthorized_charger() {
        let (mut user1, _) = TestUser::random().await;
        let (mut user2, _) = TestUser::random().await;
        let _token1 = user1.login().await.to_string();
        let token2 = user2.login().await.to_string();

        // User 1 creates a charger
        let charger_id = OsRng.try_next_u32().unwrap() as i32;
        let charger = user1.add_charger(charger_id).await;

        // User 2 creates a grouping
        let grouping = create_test_grouping(&token2, "User2 Group").await;

        let app = App::new()
            .configure(test_configure)
            .configure(configure);
        let app = test::init_service(app).await;

        // User 2 tries to add User 1's charger to their grouping
        let body = AddDeviceToGroupingSchema {
            grouping_id: grouping.id.clone(),
            device_id: charger.uuid.clone(),
        };

        let cookie = Cookie::new("access_token", token2);
        let req = test::TestRequest::post()
            .uri("/grouping/add_device")
            .cookie(cookie)
            .set_json(&body)
            .to_request();

        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status().as_u16(), 401); // Unauthorized

        // Cleanup
        delete_test_grouping_from_db(&grouping.id);
    }

    #[actix_web::test]
    async fn test_get_groupings_with_devices() {
        let (mut user, _) = TestUser::random().await;
        let token = user.login().await.to_string();

        // Create chargers
        let charger1_id = OsRng.try_next_u32().unwrap() as i32;
        let charger1 = user.add_charger(charger1_id).await;
        let charger2_id = OsRng.try_next_u32().unwrap() as i32;
        let charger2 = user.add_charger(charger2_id).await;

        // Create grouping
        let grouping = create_test_grouping(&token, "Multi-Device Group").await;

        let app = App::new()
            .configure(test_configure)
            .configure(configure);
        let app = test::init_service(app).await;

        // Add both devices to grouping
        let cookie = Cookie::new("access_token", token);

        let body1 = AddDeviceToGroupingSchema {
            grouping_id: grouping.id.clone(),
            device_id: charger1.uuid.clone(),
        };
        let req = test::TestRequest::post()
            .uri("/grouping/add_device")
            .cookie(cookie.clone())
            .set_json(&body1)
            .to_request();
        test::call_service(&app, req).await;

        let body2 = AddDeviceToGroupingSchema {
            grouping_id: grouping.id.clone(),
            device_id: charger2.uuid.clone(),
        };
        let req = test::TestRequest::post()
            .uri("/grouping/add_device")
            .cookie(cookie.clone())
            .set_json(&body2)
            .to_request();
        test::call_service(&app, req).await;

        // Get groupings
        let req = test::TestRequest::get()
            .uri("/grouping/list")
            .cookie(cookie)
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_success());

        let body: GetGroupingsResponse = test::read_body_json(resp).await;
        assert_eq!(body.groupings.len(), 1);
        assert_eq!(body.groupings[0].device_ids.len(), 2);
        assert!(body.groupings[0].device_ids.contains(&charger1.uuid));
        assert!(body.groupings[0].device_ids.contains(&charger2.uuid));

        // Cleanup
        delete_test_grouping_from_db(&grouping.id);
    }

    #[actix_web::test]
    async fn test_delete_grouping_cascades_members() {
        let (mut user, _) = TestUser::random().await;
        let token = user.login().await.to_string();

        let charger_id = OsRng.try_next_u32().unwrap() as i32;
        let charger = user.add_charger(charger_id).await;
        let grouping = create_test_grouping(&token, "Cascade Test").await;

        let app = App::new()
            .configure(test_configure)
            .configure(configure);
        let app = test::init_service(app).await;

        // Add device to grouping
        let add_body = AddDeviceToGroupingSchema {
            grouping_id: grouping.id.clone(),
            device_id: charger.uuid.clone(),
        };

        let cookie = Cookie::new("access_token", token);
        let req = test::TestRequest::post()
            .uri("/grouping/add_device")
            .cookie(cookie.clone())
            .set_json(&add_body)
            .to_request();
        test::call_service(&app, req).await;

        // Verify member exists
        assert_eq!(count_grouping_members(&grouping.id), 1);

        // Delete grouping
        use crate::routes::grouping::delete_grouping::DeleteGroupingSchema;
        let delete_body = DeleteGroupingSchema {
            grouping_id: grouping.id.clone(),
        };

        let req = test::TestRequest::delete()
            .uri("/grouping/delete")
            .cookie(cookie)
            .set_json(&delete_body)
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_success());

        // Verify members are also deleted (cascade)
        assert_eq!(count_grouping_members(&grouping.id), 0);
    }
}
