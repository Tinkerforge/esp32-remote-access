/* esp32-remote-access
 * Copyright (C) 2024 Frederic Henrichs <frederic@tinkerforge.com>
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
use db_connector::models::wg_keys::WgKey;
use diesel::{
    result::Error::NotFound, BelongingToDsl, ExpressionMethods, QueryDsl, SelectableHelper,
};
use diesel_async::RunQueryDsl as _;
use ipnetwork::IpNetwork;
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

use crate::{
    error::Error,
    firmware::supports_ipv6_tunnel,
    routes::user::get_user,
    utils::{get_connection, parse_uuid},
    AppState,
};

#[derive(Serialize, Deserialize, ToSchema)]
pub struct GetWgKeysResponseSchema {
    id: String,
    charger_id: String,
    charger_pub: String,
    #[schema(value_type = SchemaType::String)]
    charger_address: IpNetwork,
    #[schema(value_type = Vec<u32>)]
    web_private: Vec<u8>,
    #[schema(value_type = Vec<u32>)]
    psk: Vec<u8>,
    #[schema(value_type = SchemaType::String)]
    web_address: IpNetwork,
    /// Whether this device's model and firmware allow IPv6 tunnel attempts.
    #[serde(default)]
    ipv6_supported: bool,
    /// Legacy ESP32 address pair for an explicitly advertised discovery token.
    /// This is session fallback metadata; registered addresses are unchanged.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    ipv4_fallback: Option<Ipv4Fallback>,
}

#[derive(Serialize, Deserialize, ToSchema)]
pub struct Ipv4Fallback {
    #[schema(value_type = SchemaType::String)]
    web_address: IpNetwork,
    #[schema(value_type = SchemaType::String)]
    charger_address: IpNetwork,
}

fn ipv4_fallback(peer: IpNetwork, connection_no: i32) -> Option<Ipv4Fallback> {
    // Only the all-nodes discovery token gets a legacy IPv4 fallback.
    // Concrete IPv6 services have no fallback.
    if peer.ip() != "ff02::1".parse::<std::net::IpAddr>().unwrap() {
        return None;
    }
    let slot = u8::try_from(connection_no).ok()?;
    Some(Ipv4Fallback {
        web_address: IpNetwork::new(std::net::Ipv4Addr::new(10, 123, slot, 3).into(), 24).unwrap(),
        charger_address: IpNetwork::new(std::net::Ipv4Addr::new(10, 123, slot, 2).into(), 32).unwrap(),
    })
}

#[derive(Serialize, Deserialize, IntoParams)]
pub struct GetWgKeysQuery {
    cid: String,
}

#[utoipa::path(
    context_path = "/charger",
    responses(
        (status = 200, body = GetWgKeysResponseSchema),
        (status = 400, description = "Somehow got a valid jwt but the user does not exist."),
        (status = 404, description = "All keys for this charger are currently in use")
    ),
    security(
        ("jwt" = [])
    ),
    params(
        GetWgKeysQuery
    )
)]
#[get("/get_key")]
pub async fn get_key(
    state: web::Data<AppState>,
    uid: crate::models::uuid::Uuid,
    web_query: web::Query<GetWgKeysQuery>,
) -> actix_web::Result<impl Responder> {
    use db_connector::schema::wg_keys::dsl::*;

    let user = get_user(&state, uid.into()).await?;
    let cid = parse_uuid(&web_query.cid)?;

    // Fetch device keys, then compute the in-use count while holding the
    // in-memory cache lock. Both are released before we do anything else.
    let keys_in_use_count = {
        let device_key_ids: Vec<uuid::Uuid> = {
            let mut conn = get_connection(&state).await?;

            match wg_keys
                .filter(charger_id.eq(&cid))
                .select(WgKey::as_select())
                .load::<WgKey>(&mut conn)
                .await
            {
                Ok(keys) => keys.into_iter().map(|k| k.id).collect(),
                Err(NotFound) => Vec::new(),
                Err(_err) => return Err(Error::InternalError.into()),
            }
        };

        let keys_in_use_cache = state.keys_in_use.lock().await;
        device_key_ids
            .into_iter()
            .filter(|key_id| keys_in_use_cache.contains(key_id))
            .collect::<Vec<_>>()
    };

    if keys_in_use_count.len() >= 5 {
        return Err(Error::AllKeysInUse.into());
    }

    let mut conn = get_connection(&state).await?;
    use db_connector::schema::chargers;
    let key = match WgKey::belonging_to(&user)
        .filter(charger_id.eq(&cid))
        .filter(id.ne_all(keys_in_use_count))
        .inner_join(chargers::table)
        .select((
            WgKey::as_select(),
            chargers::firmware_version,
            chargers::device_type,
        ))
        .get_result::<(WgKey, String, Option<String>)>(&mut conn)
        .await
    {
        Ok(v) => Some(v),
        Err(NotFound) => None,
        Err(_err) => return Err(Error::InternalError.into()),
    };

    if let Some((key, firmware_version, device_type)) = key {
        let ipv4_fallback = ipv4_fallback(key.charger_address, key.connection_no);
        let key = GetWgKeysResponseSchema {
            id: key.id.to_string(),
            charger_id: key.charger_id.to_string(),
            charger_pub: key.charger_pub,
            charger_address: key.charger_address,
            web_private: key.web_private,
            psk: key.psk,
            web_address: key.web_address,
            ipv6_supported: supports_ipv6_tunnel(device_type.as_deref(), &firmware_version),
            ipv4_fallback,
        };
        Ok(HttpResponse::Ok().json(key))
    } else {
        Err(Error::AllKeysInUse.into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::{cookie::Cookie, test, App};

    use crate::{
        middleware::jwt::JwtMiddleware,
        routes::user::tests::{random_positive_charger_id, TestUser},
        tests::{configure, get_device_key_ids, mark_keys_as_in_use},
    };

    #[actix_web::test]
    async fn discovery_token_has_a_slot_specific_legacy_profile() {
        for (peer, slot, expected) in [
            ("ff02::1/128", 1, Some(("10.123.1.3/24", "10.123.1.2/32"))),
            ("ff02::1/128", 0, Some(("10.123.0.3/24", "10.123.0.2/32"))),
            ("ff02:0:0:0:0:0:0:1/64", 255, Some(("10.123.255.3/24", "10.123.255.2/32"))),
            ("ff02::1", -1, None), ("ff02::1", 256, None),
            ("fe80::1/128", 1, None), ("fe80:0:0:0:0:0:0:1/64", 255, None),
            ("10.123.1.2/24", 1, None), ("fd00::2/128", 1, None),
        ] {
            let profile = ipv4_fallback(peer.parse().unwrap(), slot);
            assert_eq!(profile.map(|p| (p.web_address.to_string(), p.charger_address.to_string())),
                expected.map(|(local, peer)| (local.to_owned(), peer.to_owned())));
        }
    }

    #[actix_web::test]
    async fn test_get_key() {
        let (mut user, _) = TestUser::random().await;
        user.login().await;

        let device_uid = random_positive_charger_id();
        let device = user.add_charger(device_uid).await;

        let app = App::new()
            .configure(configure)
            .wrap(JwtMiddleware)
            .service(get_key);
        let app = test::init_service(app).await;

        let req = test::TestRequest::get()
            .uri(&format!("/get_key?cid={}", device.uuid))
            .cookie(Cookie::new("access_token", user.get_access_token()))
            .to_request();

        let resp: GetWgKeysResponseSchema = test::call_and_read_body_json(&app, req).await;
        assert_eq!(resp.charger_id, device.uuid);
    }

    #[actix_web::test]
    async fn key_response_uses_current_firmware_metadata() {
        use db_connector::schema::chargers;

        let (mut user, _) = TestUser::random().await;
        user.login().await;
        let device = user.add_random_charger().await;
        let device_id: uuid::Uuid = device.uuid.parse().unwrap();
        let state = crate::tests::create_test_state(None);
        let app = test::init_service(
            App::new()
                .configure(configure)
                .app_data(state.clone())
                .wrap(JwtMiddleware)
                .service(get_key),
        )
        .await;
        let mut original_key = None;
        // Upgrade/downgrade must change session policy without rotating keys.
        for (version, device_type, expected) in [
            ("2.13.4", Some("Tinkerforge-WARP3_Charger/2.13.4"), false),
            (
                "2.13.5+1234abcd",
                Some("Tinkerforge-WARP3_Charger/2.13.5+1234abcd"),
                true,
            ),
            ("2.14.0", Some("Tinkerforge-WARP3_Charger/2.14.0"), true),
            ("2.13.4", Some("Tinkerforge-WARP3_Charger/2.13.4"), false),
            ("invalid", Some("Tinkerforge-WARP3_Charger/invalid"), false),
            ("2.14.0", None, false),
        ] {
            {
                let mut conn = get_connection(&state).await.unwrap();
                diesel::update(chargers::table.find(device_id))
                    .set((
                        chargers::firmware_version.eq(version),
                        chargers::device_type.eq(device_type),
                    ))
                    .execute(&mut conn)
                    .await
                    .unwrap();
            }
            let req = test::TestRequest::get()
                .uri(&format!("/get_key?cid={device_id}"))
                .cookie(Cookie::new("access_token", user.get_access_token()))
                .to_request();
            let response: GetWgKeysResponseSchema = test::call_and_read_body_json(&app, req).await;
            assert_eq!(response.ipv6_supported, expected, "{version}");
            let key = (
                response.id,
                response.charger_address,
                response.web_address,
                response.charger_pub,
                response.web_private,
                response.psk,
            );
            if let Some(original) = &original_key {
                assert_eq!(&key, original);
            } else {
                original_key = Some(key);
            }
        }
    }

    #[actix_web::test]
    async fn test_get_key_all_keys_in_use() {
        use std::str::FromStr;

        let (mut user, _) = TestUser::random().await;
        user.login().await;

        let device_uid = random_positive_charger_id();
        let device = user.add_charger(device_uid).await;

        let state = crate::tests::create_test_state(None);
        let device_uuid = uuid::Uuid::from_str(&device.uuid).unwrap();
        let key_ids = get_device_key_ids(&state, device_uuid).await;

        mark_keys_as_in_use(&state, key_ids).await;

        let app = App::new()
            .app_data(state)
            .wrap(JwtMiddleware)
            .service(get_key);
        let app = test::init_service(app).await;

        let req = test::TestRequest::get()
            .uri(&format!("/get_key?cid={}", device.uuid))
            .cookie(Cookie::new("access_token", user.get_access_token()))
            .to_request();

        // Should fail with 404 since all keys are in use
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_client_error());
        assert_eq!(resp.status().as_u16(), 404);
    }
    /// Registration (including the token path's shared register_charger), user
    /// addition, persistence and the keys response must never promote IPv4 to
    /// IPv6 or replace a caller's prefix. Bare firmware registrations are /32
    /// in INET; the firmware's local /24 configuration is independent of that.
    #[actix_web::test]
    async fn preserves_registered_tunnel_addresses() {
        use super::super::add::{
            add, encode_charger_uid, tests::generate_random_keys, AddChargerResponseSchema,
            AddChargerSchema, ChargerSchema,
        };
        use super::super::{allow_user::allow_user, tests::TestCharger};
        use crate::utils::get_charger_from_db;
        use base64::{engine::general_purpose::STANDARD, Engine};
        use serde_json::json;

        for (management_peer, management_local, local, peer) in [
            ("10.123.123.2", "10.123.123.3", "10.123.1.3", "10.123.1.2"),
            (
                "10.123.123.2/24",
                "10.123.123.3/24",
                "10.123.1.3/24",
                "10.123.1.2/32",
            ),
            ("fd01::2/64", "fd01::3/128", "fd00::3/64", "fd00::2/128"),
            ("fd01::2/64", "fd01::3/128", "fd00::3/128", "ff02::1/128"),
        ] {
            // Destruction order ensures the owner removes the device first.
            let (mut invited, _) = TestUser::random().await;
            let (mut owner, _) = TestUser::random().await;
            owner.login().await;
            invited.login().await;
            let state = crate::tests::create_test_state(None);
            let app = test::init_service(
                App::new()
                    .configure(configure)
                    .app_data(state.clone())
                    .wrap(JwtMiddleware)
                    .service(add)
                    .service(allow_user)
                    .service(get_key),
            )
            .await;
            let uid = random_positive_charger_id();
            let mut keys = generate_random_keys();
            for key in &mut keys {
                key.web_address = local.parse().unwrap();
                key.charger_address = peer.parse().unwrap();
            }
            let registration = AddChargerSchema {
                charger: ChargerSchema {
                    uid: encode_charger_uid(uid),
                    charger_pub: keys[0].charger_public.clone(),
                    wg_charger_ip: management_peer.parse().unwrap(),
                    wg_server_ip: management_local.parse().unwrap(),
                    psk: STANDARD.encode([3; 32]),
                },
                keys: keys.clone(),
                name: String::new(),
                note: String::new(),
            };
            let req = test::TestRequest::put()
                .uri("/add")
                .cookie(Cookie::new("access_token", owner.get_access_token()))
                .set_json(registration)
                .to_request();
            let added: AddChargerResponseSchema = test::call_and_read_body_json(&app, req).await;
            owner.devices.push(TestCharger {
                uid,
                uuid: added.charger_uuid.clone(),
                password: added.charger_password.clone(),
            });
            let charger = get_charger_from_db(added.charger_uuid.parse().unwrap(), &state)
                .await
                .unwrap();
            assert_eq!(
                charger.wg_charger_ip,
                management_peer.parse::<IpNetwork>().unwrap()
            );
            assert_eq!(
                charger.wg_server_ip,
                management_local.parse::<IpNetwork>().unwrap()
            );
            let req = test::TestRequest::put()
                .uri("/allow_user")
                .cookie(Cookie::new("access_token", owner.get_access_token()))
                .insert_header(("X-Forwarded-For", "127.0.0.1"))
                .set_json(json!({
                    "charger_id": added.charger_uuid, "charger_password": added.charger_password,
                    "email": invited.mail, "user_uuid": null,
                    "user_auth": { "LoginKey": STANDARD.encode(invited.get_login_key().await) },
                    "wg_keys": keys, "charger_name": "", "note": "",
                }))
                .to_request();
            let response = test::call_service(&app, req).await;
            assert!(response.status().is_success());
            for user in [&owner, &invited] {
                for _ in 0..2 {
                    // Returning a key must not migrate or rotate existing keys.
                    let req = test::TestRequest::get()
                        .uri(&format!("/get_key?cid={}", charger.id))
                        .cookie(Cookie::new("access_token", user.get_access_token()))
                        .to_request();
                    let response: GetWgKeysResponseSchema =
                        test::call_and_read_body_json(&app, req).await;
                    assert_eq!(response.web_address, local.parse::<IpNetwork>().unwrap());
                    assert_eq!(response.charger_address, peer.parse::<IpNetwork>().unwrap());
                    let original = keys
                        .iter()
                        .find(|k| k.charger_public == response.charger_pub)
                        .unwrap();
                    let expected_fallback = ipv4_fallback(original.charger_address, original.connection_no as i32);
                    assert_eq!(serde_json::to_value(&response.ipv4_fallback).unwrap(),
                        serde_json::to_value(expected_fallback).unwrap());
                    assert_eq!(response.web_private, original.web_private);
                    assert_eq!(response.psk, original.psk);
                }
            }
        }
    }
}
