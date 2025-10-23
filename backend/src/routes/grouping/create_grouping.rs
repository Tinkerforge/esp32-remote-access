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
}

#[derive(Serialize, Deserialize, ToSchema)]
pub struct CreateGroupingResponse {
    pub id: String,
    pub name: String,
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
    let user_uuid: uuid::Uuid = user_id.into();
    let mut conn = get_connection(&state)?;

    let grouping = web_block_unpacked(move || {
        let new_grouping = DeviceGrouping {
            id: uuid::Uuid::new_v4(),
            name: name.clone(),
            user_id: user_uuid,
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
    }))
}

#[cfg(test)]
mod tests {
    use crate::routes::{
        grouping::test_helpers::*,
        user::tests::TestUser,
    };

    #[actix_web::test]
    async fn test_create_grouping() {
        let (mut user, _) = TestUser::random().await;
        let token = user.login().await;

        let grouping_name = "Test Grouping";
        let response = create_test_grouping(token, grouping_name).await;

        assert_eq!(response.name, grouping_name);
        assert!(!response.id.is_empty());

        // Verify grouping exists in database
        let db_grouping = get_grouping_from_db(&response.id);
        assert!(db_grouping.is_some());
        assert_eq!(db_grouping.unwrap().name, grouping_name);

        // Cleanup
        delete_test_grouping_from_db(&response.id);
    }
}
