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

use actix_web::{get, web, HttpResponse, Responder};
use db_connector::models::{
    device_grouping_members::DeviceGroupingMember, device_groupings::DeviceGrouping,
};
use diesel::prelude::*;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::{
    error::Error,
    utils::{get_connection, web_block_unpacked},
    AppState,
};

#[derive(Serialize, Deserialize, ToSchema)]
pub struct GroupingInfo {
    pub id: String,
    pub name: String,
    pub device_ids: Vec<String>,
}

#[derive(Serialize, Deserialize, ToSchema)]
pub struct GetGroupingsResponse {
    pub groupings: Vec<GroupingInfo>,
}

/// Get all device groupings for the current user
#[utoipa::path(
    context_path = "/grouping",
    responses(
        (status = 200, description = "List of groupings", body = GetGroupingsResponse),
        (status = 500, description = "Internal server error")
    ),
    security(
        ("jwt" = [])
    )
)]
#[get("/list")]
pub async fn get_groupings(
    state: web::Data<AppState>,
    user_id: crate::models::uuid::Uuid,
) -> actix_web::Result<impl Responder> {
    use db_connector::schema::device_grouping_members::dsl as members;
    use db_connector::schema::device_groupings::dsl as groupings;

    let user_uuid: uuid::Uuid = user_id.into();
    let mut conn = get_connection(&state)?;

    let groupings_info = web_block_unpacked(move || {
        // Get all groupings for the user
        let user_groupings: Vec<DeviceGrouping> = match groupings::device_groupings
            .filter(groupings::user_id.eq(user_uuid))
            .select(DeviceGrouping::as_select())
            .load(&mut conn)
        {
            Ok(gs) => gs,
            Err(_err) => return Err(Error::InternalError),
        };

        // For each grouping, get its members
        let mut result = Vec::new();
        for grouping in user_groupings {
            let grouping_members: Vec<DeviceGroupingMember> = match members::device_grouping_members
                .filter(members::grouping_id.eq(grouping.id))
                .select(DeviceGroupingMember::as_select())
                .load(&mut conn)
            {
                Ok(ms) => ms,
                Err(_err) => return Err(Error::InternalError),
            };

            result.push(GroupingInfo {
                id: grouping.id.to_string(),
                name: grouping.name,
                device_ids: grouping_members
                    .iter()
                    .map(|m| m.charger_id.to_string())
                    .collect(),
            });
        }

        Ok(result)
    })
    .await?;

    Ok(HttpResponse::Ok().json(GetGroupingsResponse {
        groupings: groupings_info,
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
    async fn test_get_groupings_empty() {
        let (mut user, _) = TestUser::random().await;
        let token = user.login().await;

        let app = App::new().configure(test_configure).configure(configure);
        let app = test::init_service(app).await;

        let cookie = Cookie::new("access_token", token);
        let req = test::TestRequest::get()
            .uri("/grouping/list")
            .cookie(cookie)
            .to_request();

        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_success());

        let body: GetGroupingsResponse = test::read_body_json(resp).await;
        assert_eq!(body.groupings.len(), 0);
    }

    #[actix_web::test]
    async fn test_get_groupings_with_data() {
        let (mut user, _) = TestUser::random().await;
        let token = user.login().await;

        // Create two groupings
        let grouping1 = create_test_grouping(token, "Grouping 1").await;
        let grouping2 = create_test_grouping(token, "Grouping 2").await;

        let app = App::new().configure(test_configure).configure(configure);
        let app = test::init_service(app).await;

        let cookie = Cookie::new("access_token", token);
        let req = test::TestRequest::get()
            .uri("/grouping/list")
            .cookie(cookie)
            .to_request();

        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_success());

        let body: GetGroupingsResponse = test::read_body_json(resp).await;
        assert_eq!(body.groupings.len(), 2);

        // Cleanup
        delete_test_grouping_from_db(&grouping1.id);
        delete_test_grouping_from_db(&grouping2.id);
    }
}
