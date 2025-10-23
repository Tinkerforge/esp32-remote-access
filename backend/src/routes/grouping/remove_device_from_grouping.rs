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

use actix_web::{delete, web, HttpResponse, Responder};
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
pub struct RemoveDeviceFromGroupingSchema {
    pub grouping_id: String,
    pub charger_id: String,
}

/// Remove a device from a grouping
#[utoipa::path(
    context_path = "/grouping",
    request_body = RemoveDeviceFromGroupingSchema,
    responses(
        (status = 200, description = "Device removed from grouping successfully"),
        (status = 400, description = "Invalid input"),
        (status = 401, description = "Unauthorized - user does not own this grouping"),
        (status = 404, description = "Grouping or device not found in grouping"),
        (status = 500, description = "Internal server error")
    ),
    security(
        ("jwt" = [])
    )
)]
#[delete("/remove_device")]
pub async fn remove_device_from_grouping(
    state: web::Data<AppState>,
    payload: web::Json<RemoveDeviceFromGroupingSchema>,
    user_id: crate::models::uuid::Uuid,
) -> actix_web::Result<impl Responder> {
    use db_connector::schema::device_grouping_members::dsl as members;
    use db_connector::schema::device_groupings::dsl as groupings;

    let grouping_uuid = parse_uuid(&payload.grouping_id)?;
    let charger_uuid = parse_uuid(&payload.charger_id)?;
    let user_uuid: uuid::Uuid = user_id.into();
    let mut conn = get_connection(&state)?;

    web_block_unpacked(move || {
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

        // Delete the member
        let deleted = match diesel::delete(
            members::device_grouping_members
                .filter(members::grouping_id.eq(grouping_uuid))
                .filter(members::charger_id.eq(charger_uuid)),
        )
        .execute(&mut conn)
        {
            Ok(count) => count,
            Err(_err) => return Err(Error::InternalError),
        };

        // Check if any rows were deleted
        if deleted == 0 {
            return Err(Error::ChargerDoesNotExist);
        }

        Ok(())
    })
    .await?;

    Ok(HttpResponse::Ok().finish())
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
                add_device_to_grouping::AddDeviceToGroupingSchema,
                configure, test_helpers::*,
            },
            user::tests::TestUser,
        },
        tests::configure as test_configure,
    };

    #[actix_web::test]
    async fn test_remove_device_from_grouping() {
        let (mut user, _) = TestUser::random().await;
        let token = user.login().await.to_string();

        let charger_id = OsRng.try_next_u32().unwrap() as i32;
        let charger = user.add_charger(charger_id).await;
        let grouping = create_test_grouping(&token, "Test Group").await;

        let app = App::new()
            .configure(test_configure)
            .configure(configure);
        let app = test::init_service(app).await;

        // Add device to grouping
        let add_body = AddDeviceToGroupingSchema {
            grouping_id: grouping.id.clone(),
            charger_id: charger.uuid.clone(),
        };

        let cookie = Cookie::new("access_token", token);
        let req = test::TestRequest::post()
            .uri("/grouping/add_device")
            .cookie(cookie.clone())
            .set_json(&add_body)
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_success());

        // Remove device from grouping
        let remove_body = RemoveDeviceFromGroupingSchema {
            grouping_id: grouping.id.clone(),
            charger_id: charger.uuid.clone(),
        };

        let req = test::TestRequest::delete()
            .uri("/grouping/remove_device")
            .cookie(cookie)
            .set_json(&remove_body)
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_success());

        // Verify member is removed
        let member_count = count_grouping_members(&grouping.id);
        assert_eq!(member_count, 0);

        // Cleanup
        delete_test_grouping_from_db(&grouping.id);
    }

    #[actix_web::test]
    async fn test_remove_device_from_grouping_not_in_group() {
        let (mut user, _) = TestUser::random().await;
        let token = user.login().await.to_string();

        let charger_id = OsRng.try_next_u32().unwrap() as i32;
        let charger = user.add_charger(charger_id).await;
        let grouping = create_test_grouping(&token, "Test Group").await;

        let app = App::new()
            .configure(test_configure)
            .configure(configure);
        let app = test::init_service(app).await;

        // Try to remove device that was never added
        let body = RemoveDeviceFromGroupingSchema {
            grouping_id: grouping.id.clone(),
            charger_id: charger.uuid.clone(),
        };

        let cookie = Cookie::new("access_token", token);
        let req = test::TestRequest::delete()
            .uri("/grouping/remove_device")
            .cookie(cookie)
            .set_json(&body)
            .to_request();

        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status().as_u16(), 400); // Bad Request (not found)

        // Cleanup
        delete_test_grouping_from_db(&grouping.id);
    }
}
