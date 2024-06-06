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

use std::time::{SystemTime, UNIX_EPOCH};

use actix_web::{error::ErrorUnauthorized, put, web, HttpRequest, HttpResponse, Responder};
use db_connector::models::wg_keys::WgKey;
use diesel::prelude::*;
use ipnetwork::IpNetwork;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::{
    error::Error,
    routes::charger::add::get_charger_from_db,
    utils::{get_connection, web_block_unpacked},
    ws_udp_bridge::open_connection,
    AppState, BridgeState,
};

use super::charger::add::password_matches;

#[derive(Serialize, Deserialize, ToSchema)]
pub struct ManagementSchema {
    id: i32,
    password: String,
    data: ManagementDataVersion,
}

#[derive(Serialize, Deserialize, ToSchema)]
pub enum ManagementDataVersion {
    V1(ManagementDataVersion1),
}

#[derive(Serialize, Deserialize, ToSchema)]
pub struct ManagementDataVersion1 {
    port: i16,
}

#[derive(Serialize, Deserialize, ToSchema)]
pub struct ManagementResponseSchema {
    time: u64,
}

/// Route for the charger to be identifiable via the ip.
#[utoipa::path(
    request_body = ManagementSchema,
    responses(
        (status = 200, description = "Identification was successful", body = ManagementResponseSchema),
        (status = 400, description = "Got no valid ip address for the charger"),
        (status = 401, description = "The logged in user is not the owner of the charger")
    )
)]
#[put("/management")]
pub async fn management(
    req: HttpRequest,
    state: web::Data<AppState>,
    data: web::Json<ManagementSchema>,
    bridge_state: web::Data<BridgeState>,
) -> actix_web::Result<impl Responder> {
    use db_connector::schema::chargers::dsl as chargers;

    let info = req.connection_info();
    let ip = info.realip_remote_addr();

    if ip.is_none() {
        return Err(Error::NoValidIp.into());
    }

    let ip = ip.unwrap();

    let charger = get_charger_from_db(data.id, &state).await?;

    if !password_matches(data.password.clone(), charger.password.clone())? {
        return Err(ErrorUnauthorized(""));
    }

    let ip: IpNetwork = match ip.parse() {
        Ok(ip) => ip,
        Err(_err) => {
            log::error!("Error while parsing ip: {}", _err);
            return Err(Error::InternalError.into());
        }
    };

    let mut conn = get_connection(&state)?;
    let keys_in_use: Vec<WgKey> = web_block_unpacked(move || {
        use db_connector::schema::wg_keys::dsl::*;

        match WgKey::belonging_to(&charger)
            .filter(in_use.eq(true))
            .load(&mut conn)
        {
            Ok(k) => Ok(k),
            Err(_err) => Err(Error::InternalError),
        }
    })
    .await?;

    {
        let charger_map = bridge_state.charger_management_map_with_id.lock().unwrap();
        if let Some(c) = charger_map.get(&data.id) {
            {
                let mut charger = c.lock().unwrap();
                charger.reset_out_sequence();
                charger.reset();
            }
            for key in keys_in_use.iter() {
                open_connection(
                    key.connection_no,
                    key.charger_id,
                    c.clone(),
                    bridge_state.port_discovery.clone(),
                )?;
            }
        }
    }

    let mut conn = get_connection(&state)?;
    web_block_unpacked(move || {
        match diesel::update(chargers::chargers)
            .filter(chargers::id.eq(data.id))
            .set(chargers::last_ip.eq(Some(ip)))
            .execute(&mut conn)
        {
            Ok(_) => Ok(()),
            Err(_err) => {
                log::error!("Error while updating charger: {}", _err);
                return Err(Error::InternalError.into());
            }
        }
    })
    .await?;

    let time = match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(time) => time,
        Err(err) => {
            log::error!("Error while getting current time: {}", err);
            return Err(Error::InternalError.into());
        }
    };

    let time = time.as_secs();
    let resp = ManagementResponseSchema { time };

    Ok(HttpResponse::Ok().json(resp))
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::{cookie::Cookie, test, App};
    use rand::distributions::{Alphanumeric, DistString};

    use crate::{routes::user::tests::TestUser, tests::configure};

    #[actix_web::test]
    async fn test_management() {
        let (mut user, _) = TestUser::random().await;
        user.login().await;
        let (charger, pass) = user.add_random_charger().await;

        let app = App::new().configure(configure).service(management);
        let app = test::init_service(app).await;

        let data = ManagementDataVersion::V1(ManagementDataVersion1 { port: 0 });

        let body = ManagementSchema {
            id: charger,
            password: pass,
            data,
        };
        let req = test::TestRequest::put()
            .uri("/management")
            .append_header(("X-Forwarded-For", "123.123.123.3"))
            .cookie(Cookie::new("X-Forwarded-For", "123.123.123.3"))
            .set_json(body)
            .to_request();
        let resp = test::call_service(&app, req).await;

        println!("{:?}", resp);
        println!("{:?}", resp.response().body());
        assert!(resp.status().is_success());
    }

    #[actix_web::test]
    async fn test_wrong_password() {
        let (mut user, _) = TestUser::random().await;
        user.login().await;
        let (charger, _) = user.add_random_charger().await;

        let app = App::new().configure(configure).service(management);
        let app = test::init_service(app).await;

        let data = ManagementDataVersion::V1(ManagementDataVersion1 { port: 0 });
        let body = ManagementSchema {
            id: charger,
            password: Alphanumeric.sample_string(&mut rand::thread_rng(), 32),
            data,
        };
        let req = test::TestRequest::put()
            .uri("/management")
            .append_header(("X-Forwarded-For", "123.123.123.3"))
            .set_json(body)
            .to_request();
        let resp = test::call_service(&app, req).await;

        println!("{:?}", resp);
        println!("{:?}", resp.response().body());
        assert!(resp.status().is_client_error());
        assert_eq!(resp.status().as_u16(), 401);
    }
}
