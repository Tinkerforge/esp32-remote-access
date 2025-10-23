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
pub struct DeleteGroupingSchema {
    pub grouping_id: String,
}

/// Delete a device grouping
#[utoipa::path(
    context_path = "/grouping",
    request_body = DeleteGroupingSchema,
    responses(
        (status = 200, description = "Grouping deleted successfully"),
        (status = 400, description = "Invalid grouping ID"),
        (status = 401, description = "Unauthorized - user does not own this grouping"),
        (status = 404, description = "Grouping not found"),
        (status = 500, description = "Internal server error")
    ),
    security(
        ("jwt" = [])
    )
)]
#[delete("/delete")]
pub async fn delete_grouping(
    state: web::Data<AppState>,
    payload: web::Json<DeleteGroupingSchema>,
    user_id: crate::models::uuid::Uuid,
) -> actix_web::Result<impl Responder> {
    use db_connector::schema::device_groupings::dsl as groupings;

    let grouping_uuid = parse_uuid(&payload.grouping_id)?;
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

        // Delete the grouping (cascade will handle members)
        match diesel::delete(groupings::device_groupings.find(grouping_uuid)).execute(&mut conn) {
            Ok(_) => Ok(()),
            Err(_err) => Err(Error::InternalError),
        }
    })
    .await?;

    Ok(HttpResponse::Ok().finish())
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
    async fn test_delete_grouping() {
        let (mut user, _) = TestUser::random().await;
        let token = user.login().await;

        let grouping = create_test_grouping(token, "To Delete").await;

        let app = App::new()
            .configure(test_configure)
            .configure(configure);
        let app = test::init_service(app).await;

        let body = DeleteGroupingSchema {
            grouping_id: grouping.id.clone(),
        };

        let cookie = Cookie::new("access_token", token);
        let req = test::TestRequest::delete()
            .uri("/grouping/delete")
            .cookie(cookie)
            .set_json(&body)
            .to_request();

        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_success());

        // Verify grouping is deleted
        let db_grouping = get_grouping_from_db(&grouping.id);
        assert!(db_grouping.is_none());
    }

    #[actix_web::test]
    async fn test_delete_grouping_unauthorized() {
        let (mut user1, _) = TestUser::random().await;
        let (mut user2, _) = TestUser::random().await;
        let token1 = user1.login().await;
        let token2 = user2.login().await;

        // User 1 creates a grouping
        let grouping = create_test_grouping(token1, "User1 Grouping").await;

        let app = App::new()
            .configure(test_configure)
            .configure(configure);
        let app = test::init_service(app).await;

        // User 2 tries to delete it
        let body = DeleteGroupingSchema {
            grouping_id: grouping.id.clone(),
        };

        let cookie = Cookie::new("access_token", token2);
        let req = test::TestRequest::delete()
            .uri("/grouping/delete")
            .cookie(cookie)
            .set_json(&body)
            .to_request();

        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status().as_u16(), 401); // Unauthorized

        // Verify grouping still exists
        let db_grouping = get_grouping_from_db(&grouping.id);
        assert!(db_grouping.is_some());

        // Cleanup
        delete_test_grouping_from_db(&grouping.id);
    }
}
