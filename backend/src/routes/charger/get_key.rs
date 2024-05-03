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
use diesel::{prelude::*, result::Error::NotFound};
use ipnetwork::IpNetwork;
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

use crate::{
    error::Error,
    routes::user::get_user,
    utils::{get_connection, web_block_unpacked},
    AppState,
};

#[derive(Serialize, Deserialize, ToSchema)]
pub struct GetWgKeysResponseSchema {
    id: String,
    charger_id: i32,
    charger_pub: String,
    #[schema(value_type = SchemaType::String)]
    charger_address: IpNetwork,
    #[schema(value_type = Vec<u32>)]
    web_private: Vec<u8>,
    #[schema(value_type = Vec<u32>)]
    web_private_nonce: Vec<u8>,
    #[schema(value_type = SchemaType::String)]
    web_address: IpNetwork,
}

#[derive(Serialize, Deserialize, IntoParams)]
pub struct GetWgKeysQuery {
    cid: i32,
}

#[utoipa::path(
    context_path = "/charger",
    responses(
        (status = 200, body = GetWgKeysSchema),
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

    let mut conn = get_connection(&state)?;
    let key: Option<WgKey> = web_block_unpacked(move || {
        match WgKey::belonging_to(&user)
            .filter(charger_id.eq(&web_query.cid))
            .filter(in_use.eq(false))
            .select(WgKey::as_select())
            .get_result(&mut conn)
        {
            Ok(v) => Ok(Some(v)),
            Err(NotFound) => Ok(None),
            Err(_err) => Err(Error::InternalError),
        }
    })
    .await?;

    if let Some(key) = key {
        let key = GetWgKeysResponseSchema {
            id: key.id.to_string(),
            charger_id: key.charger_id,
            charger_pub: key.charger_pub,
            charger_address: key.charger_address,
            web_private: key.web_private,
            web_private_nonce: key.web_private_nonce,
            web_address: key.web_address,
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
    use db_connector::test_connection_pool;
    use rand::RngCore;
    use rand_core::OsRng;

    use crate::{middleware::jwt::JwtMiddleware, routes::user::tests::TestUser, tests::configure};

    #[actix_web::test]
    async fn test_get_key() {
        let (mut user, _) = TestUser::random().await;
        user.login().await;

        let charger = OsRng.next_u32() as i32;
        user.add_charger(charger).await;

        let app = App::new()
            .configure(configure)
            .wrap(JwtMiddleware)
            .service(get_key);
        let app = test::init_service(app).await;

        let req = test::TestRequest::get()
            .uri(&format!("/get_key?cid={}", charger.clone()))
            .cookie(Cookie::new("access_token", user.get_access_token()))
            .to_request();

        let resp: GetWgKeysResponseSchema = test::call_and_read_body_json(&app, req).await;
        assert_eq!(resp.charger_id, charger);
    }

    #[actix_web::test]
    async fn test_get_key_none_left() {
        use db_connector::schema::wg_keys::dsl::*;

        let (mut user, _) = TestUser::random().await;
        user.login().await;

        let charger = OsRng.next_u32() as i32;
        user.add_charger(charger).await;

        let pool = test_connection_pool();
        let mut conn = pool.get().unwrap();

        diesel::update(wg_keys)
            .filter(charger_id.eq(&charger))
            .set(in_use.eq(true))
            .execute(&mut conn)
            .unwrap();

        let app = App::new()
            .configure(configure)
            .wrap(JwtMiddleware)
            .service(get_key);
        let app = test::init_service(app).await;

        let req = test::TestRequest::get()
            .uri(&format!("/get_key?cid={}", charger.clone()))
            .cookie(Cookie::new("access_token", user.get_access_token()))
            .to_request();

        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_client_error());
        assert_eq!(resp.status().as_u16(), 404);
    }
}
