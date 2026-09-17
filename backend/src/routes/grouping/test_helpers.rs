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

use super::*;
use actix_web::{cookie::Cookie, test, App};
use db_connector::{models::device_groupings::DeviceGrouping, test_connection_pool};
use diesel::{ExpressionMethods, QueryDsl, SelectableHelper};
use diesel_async::RunQueryDsl as _AsyncRunQueryDsl;

use crate::tests::configure as test_configure;
use create_grouping::{CreateGroupingResponse, CreateGroupingSchema};

/// Helper function to create a test grouping
pub async fn create_test_grouping(access_token: &str, name: &str) -> CreateGroupingResponse {
    let app = App::new().configure(test_configure).configure(configure);
    let app = test::init_service(app).await;

    let body = CreateGroupingSchema {
        name: name.to_string(),
        is_default: false,
    };

    let cookie = Cookie::new("access_token", access_token);
    let req = test::TestRequest::post()
        .uri("/grouping/create")
        .cookie(cookie)
        .set_json(&body)
        .to_request();

    let resp = test::call_service(&app, req).await;
    assert!(resp.status().is_success(), "Failed to create grouping");

    test::read_body_json(resp).await
}

/// Helper function to clean up test grouping from database
pub async fn delete_test_grouping_from_db(grouping_id: &str) {
    use db_connector::schema::device_groupings::dsl::*;

    let pool = test_connection_pool();
    let mut conn = pool.get().await.unwrap();
    let uuid_val = uuid::Uuid::parse_str(grouping_id).unwrap();

    diesel::delete(device_groupings.filter(id.eq(uuid_val)))
        .execute(&mut conn)
        .await
        .ok();
}

/// Helper function to get grouping from database
pub async fn get_grouping_from_db(grouping_id: &str) -> Option<DeviceGrouping> {
    use db_connector::schema::device_groupings::dsl::*;

    let pool = test_connection_pool();
    let mut conn = pool.get().await.unwrap();
    let uuid_val = uuid::Uuid::parse_str(grouping_id).unwrap();

    device_groupings
        .filter(id.eq(uuid_val))
        .select(DeviceGrouping::as_select())
        .first::<DeviceGrouping>(&mut conn)
        .await
        .ok()
}

/// Helper function to count grouping members
pub async fn count_grouping_members(grouping_id_str: &str) -> i64 {
    use db_connector::schema::device_grouping_members::dsl::*;

    let pool = test_connection_pool();
    let mut conn = pool.get().await.unwrap();
    let uuid_val = uuid::Uuid::parse_str(grouping_id_str).unwrap();

    device_grouping_members
        .filter(grouping_id.eq(uuid_val))
        .count()
        .get_result(&mut conn)
        .await
        .unwrap_or(0)
}
