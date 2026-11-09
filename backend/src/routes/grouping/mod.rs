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

pub mod add_device_to_grouping;
pub mod create_grouping;
pub mod delete_grouping;
pub mod edit_grouping;
pub mod get_groupings;
pub mod remove_device_from_grouping;

#[cfg(test)]
pub(crate) mod test_helpers;

use crate::middleware::jwt::JwtMiddleware;
use actix_web::web;

pub fn configure(cfg: &mut web::ServiceConfig) {
    let scope = web::scope("/grouping")
        .wrap(JwtMiddleware)
        .service(create_grouping::create_grouping)
        .service(delete_grouping::delete_grouping)
        .service(edit_grouping::edit_grouping)
        .service(add_device_to_grouping::add_device_to_grouping)
        .service(remove_device_from_grouping::remove_device_from_grouping)
        .service(get_groupings::get_groupings);
    cfg.service(scope);
}

async fn notify_groupings_changed(
    user_id: uuid::Uuid,
    bridge_state: &web::Data<crate::BridgeState<'_>>,
) {
    bridge_state
        .state_update_clients
        .send(
            user_id,
            crate::routes::charger::get_devices::StateUpdateMessage::GroupingsChanged,
        )
        .await;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        routes::{
            charger::get_devices::StateUpdateMessage,
            user::tests::{get_test_uuid, TestUser},
        },
        tests::{configure as test_configure, create_test_bridge_state},
    };
    use actix_web::{cookie::Cookie, test, App};

    #[actix_web::test]
    async fn all_grouping_mutations_notify_only_the_owners_tabs() {
        let (mut owner, _) = TestUser::random().await;
        let (mut other, _) = TestUser::random().await;
        owner.login().await;
        other.login().await;
        let device = owner.add_random_charger().await;
        let uid = get_test_uuid(&owner.mail).await.unwrap();
        let bridge = create_test_bridge_state(None);
        let (_, mut first) = bridge.state_update_clients.subscribe(uid).await;
        let (_, mut second) = bridge.state_update_clients.subscribe(uid).await;
        let (_, mut unrelated) = bridge
            .state_update_clients
            .subscribe(get_test_uuid(&other.mail).await.unwrap())
            .await;
        let app = test::init_service(
            App::new()
                .configure(test_configure)
                .app_data(bridge)
                .configure(configure),
        )
        .await;

        let req = test::TestRequest::post()
            .uri("/grouping/create")
            .cookie(Cookie::new("access_token", owner.get_access_token()))
            .set_json(serde_json::json!({"name": "first", "is_default": true}))
            .to_request();
        let response: create_grouping::CreateGroupingResponse =
            test::call_and_read_body_json(&app, req).await;
        let grouping_id = response.id;
        for receiver in [&mut first, &mut second] {
            assert!(matches!(
                receiver.try_recv().unwrap(),
                StateUpdateMessage::GroupingsChanged
            ));
        }

        for (method, path, body) in [
            (
                actix_web::http::Method::PUT,
                "/grouping/edit",
                serde_json::json!({"grouping_id": grouping_id, "name": "renamed", "is_default": false}),
            ),
            (
                actix_web::http::Method::POST,
                "/grouping/add_device",
                serde_json::json!({"grouping_id": grouping_id, "device_id": device.uuid}),
            ),
            (
                actix_web::http::Method::DELETE,
                "/grouping/remove_device",
                serde_json::json!({"grouping_id": grouping_id, "device_id": device.uuid}),
            ),
            (
                actix_web::http::Method::DELETE,
                "/grouping/delete",
                serde_json::json!({"grouping_id": grouping_id}),
            ),
        ] {
            // Rejected requests must not announce a change to either account.
            let req = test::TestRequest::default()
                .method(method.clone())
                .uri(path)
                .cookie(Cookie::new("access_token", other.get_access_token()))
                .set_json(&body)
                .to_request();
            assert!(test::call_service(&app, req)
                .await
                .status()
                .is_client_error());
            assert!(first.try_recv().is_err());
            assert!(second.try_recv().is_err());
            assert!(unrelated.try_recv().is_err());

            let req = test::TestRequest::default()
                .method(method)
                .uri(path)
                .cookie(Cookie::new("access_token", owner.get_access_token()))
                .set_json(&body)
                .to_request();
            assert!(test::call_service(&app, req).await.status().is_success());
            for receiver in [&mut first, &mut second] {
                assert!(matches!(
                    receiver.try_recv().unwrap(),
                    StateUpdateMessage::GroupingsChanged
                ));
                assert!(receiver.try_recv().is_err());
            }
            assert!(unrelated.try_recv().is_err());
        }
    }
}
