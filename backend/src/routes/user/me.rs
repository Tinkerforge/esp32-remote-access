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
use db_connector::models::{allowed_users::AllowedUser, chargers::Charger};
use diesel::prelude::*;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::{
    error::Error,
    routes::user::get_user,
    utils::{get_connection, web_block_unpacked},
    AppState,
};

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct UserInfo {
    pub id: String,
    pub name: String,
    pub email: String,
    pub has_old_charger: bool,
}

/// Get information about the currently logged in user.
#[utoipa::path(
    context_path = "/user",
    responses(
        (status = 200, description = "", body = UserInfo),
        (status = 400, description = "The jwt token was somehow valid but contained a non valid uuid.")
    ),
    security(
        ("jwt" = [])
    )
)]
#[get("/me")]
async fn me(
    state: web::Data<AppState>,
    id: crate::models::uuid::Uuid,
) -> Result<impl Responder, actix_web::Error> {
    let user = get_user(&state, id.clone().into()).await?;

    let id: uuid::Uuid = id.into();
    let mut conn = get_connection(&state)?;
    let allowed_users = web_block_unpacked(move || {
        use db_connector::schema::allowed_users::dsl as au;

        match au::allowed_users
            .filter(au::user_id.eq(id))
            .select(AllowedUser::as_select())
            .load::<AllowedUser>(&mut conn)
        {
            Ok(allowed_users) => Ok(allowed_users),
            Err(_e) => Err(Error::InternalError),
        }
    })
    .await?;

    let mut conn = get_connection(&state)?;
    let chargers = web_block_unpacked(move || {
        use db_connector::schema::chargers::dsl as c;

        let ids = allowed_users
            .iter()
            .map(|au| au.charger_id)
            .collect::<Vec<_>>();
        match c::chargers
            .filter(c::id.eq_any(ids))
            .select(Charger::as_select())
            .load::<Charger>(&mut conn)
        {
            Ok(chargers) => Ok(chargers),
            Err(_e) => Err(Error::InternalError),
        }
    })
    .await?;

    let mut has_old_charger = false;
    for charger in chargers.into_iter() {
        if let Ok(version) = semver::Version::parse(&charger.firmware_version) {
            let required_version = semver::Version::new(2, 6, 7);
            if version < required_version {
                has_old_charger = true;
            }
        }
    }

    let response = UserInfo {
        id: user.id.to_string(),
        name: user.name,
        email: user.email,
        has_old_charger,
    };
    Ok(HttpResponse::Ok().json(response))
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use actix_web::{cookie::Cookie, test, App};
    use db_connector::models::users::User;

    use crate::{
        defer,
        routes::auth::{
            login::tests::verify_and_login_user,
            register::tests::{create_user, delete_user},
        },
        tests::configure,
    };

    pub fn get_test_user(mail: &str) -> User {
        use db_connector::schema::users::dsl::*;

        let pool = db_connector::test_connection_pool();
        let mut conn = pool.get().unwrap();
        users
            .filter(email.eq(mail))
            .select(User::as_select())
            .get_result(&mut conn)
            .unwrap()
    }

    #[actix_web::test]
    async fn test_me() {
        let mail = "me@test.invalid";
        let key = create_user(mail).await;
        defer!(delete_user(mail));

        let app = App::new()
            .configure(configure)
            .service(me)
            .wrap(crate::middleware::jwt::JwtMiddleware);
        let app = test::init_service(app).await;

        let (token, _) = verify_and_login_user(mail, key).await;
        let req = test::TestRequest::get()
            .uri("/me")
            .cookie(Cookie::new("access_token", token))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_success());

        let body: UserInfo = test::read_body_json(resp).await;
        assert_eq!(body.email, mail);
    }

    use crate::routes::charger::tests::TestCharger;

    #[actix_web::test]
    async fn test_old_firmware_version() {
        let mail = "old_firmware@test.invalid";
        let key = create_user(mail).await;
        defer!(delete_user(mail));

        // Add charger with old firmware version
        let user = get_test_user(mail);
        let uid = rand::random::<i32>();
        let charger = TestCharger {
            uid,
            password: "password".to_string(),
            uuid: uuid::Uuid::new_v4().to_string(),
        };
        let pool = db_connector::test_connection_pool();
        let mut conn = pool.get().unwrap();

        use db_connector::models::allowed_users::AllowedUser;
        use db_connector::models::chargers::Charger;
        use db_connector::schema::allowed_users::dsl as au;
        use db_connector::schema::chargers::dsl as c;
        use uuid::Uuid;

        // Insert test charger with old firmware
        let charger_id = Uuid::new_v4();
        let test_charger = Charger {
            id: charger_id,
            uid: charger.uid,
            password: charger.password,
            name: None,
            charger_pub: "".to_string(),
            management_private: "".to_string(),
            wg_charger_ip: "0.0.0.0/0".parse().unwrap(),
            wg_server_ip: "0.0.0.0/0".parse().unwrap(),
            psk: "".to_string(),
            webinterface_port: 0,
            firmware_version: "2.6.6".to_string(), // Old version
        };
        diesel::insert_into(c::chargers)
            .values(&test_charger)
            .execute(&mut conn)
            .unwrap();

        // Add allowed_user entry
        let allowed_user = AllowedUser {
            id: Uuid::new_v4(),
            user_id: user.id,
            charger_id,
            charger_uid: charger.uid,
            valid: true,
            note: None,
            name: None,
        };
        diesel::insert_into(au::allowed_users)
            .values(&allowed_user)
            .execute(&mut conn)
            .unwrap();

        let app = App::new()
            .configure(configure)
            .service(me)
            .wrap(crate::middleware::jwt::JwtMiddleware);
        let app = test::init_service(app).await;

        let (token, _) = verify_and_login_user(mail, key).await;
        let req = test::TestRequest::get()
            .uri("/me")
            .cookie(Cookie::new("access_token", token))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_success());

        let body: UserInfo = test::read_body_json(resp).await;
        assert!(body.has_old_charger);
    }

    #[actix_web::test]
    async fn test_new_firmware_version() {
        let mail = "new_firmware@test.invalid";
        let key = create_user(mail).await;
        defer!(delete_user(mail));

        // Add charger with new firmware version
        let user = get_test_user(mail);
        let uid = rand::random::<i32>();
        let charger = TestCharger {
            uid,
            password: "password".to_string(),
            uuid: uuid::Uuid::new_v4().to_string(),
        };
        let pool = db_connector::test_connection_pool();
        let mut conn = pool.get().unwrap();

        use db_connector::models::allowed_users::AllowedUser;
        use db_connector::models::chargers::Charger;
        use db_connector::schema::allowed_users::dsl as au;
        use db_connector::schema::chargers::dsl as c;
        use uuid::Uuid;

        // Insert test charger with new firmware
        let charger_id = Uuid::new_v4();
        let test_charger = Charger {
            id: charger_id,
            uid: charger.uid,
            password: charger.password,
            name: None,
            charger_pub: "".to_string(),
            management_private: "".to_string(),
            wg_charger_ip: "0.0.0.0/0".parse().unwrap(),
            wg_server_ip: "0.0.0.0/0".parse().unwrap(),
            psk: "".to_string(),
            webinterface_port: 0,
            firmware_version: "2.6.8".to_string(), // Newer version
        };
        diesel::insert_into(c::chargers)
            .values(&test_charger)
            .execute(&mut conn)
            .unwrap();

        // Add allowed_user entry
        let allowed_user = AllowedUser {
            id: Uuid::new_v4(),
            user_id: user.id,
            charger_id,
            charger_uid: charger.uid,
            valid: true,
            note: None,
            name: None,
        };
        diesel::insert_into(au::allowed_users)
            .values(&allowed_user)
            .execute(&mut conn)
            .unwrap();

        let app = App::new()
            .configure(configure)
            .service(me)
            .wrap(crate::middleware::jwt::JwtMiddleware);
        let app = test::init_service(app).await;

        let (token, _) = verify_and_login_user(mail, key).await;
        let req = test::TestRequest::get()
            .uri("/me")
            .cookie(Cookie::new("access_token", token))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_success());

        let body: UserInfo = test::read_body_json(resp).await;
        assert!(!body.has_old_charger);
    }

    #[actix_web::test]
    async fn test_invalid_firmware_version() {
        let mail = "invalid_firmware@test.invalid";
        let key = create_user(mail).await;
        defer!(delete_user(mail));

        // Add charger with invalid firmware version
        let user = get_test_user(mail);
        let uid = rand::random::<i32>();
        let charger = TestCharger {
            uid,
            password: "password".to_string(),
            uuid: uuid::Uuid::new_v4().to_string(),
        };
        let pool = db_connector::test_connection_pool();
        let mut conn = pool.get().unwrap();

        use db_connector::models::allowed_users::AllowedUser;
        use db_connector::models::chargers::Charger;
        use db_connector::schema::allowed_users::dsl as au;
        use db_connector::schema::chargers::dsl as c;
        use uuid::Uuid;

        // Insert test charger with invalid firmware version
        let charger_id = Uuid::new_v4();
        let test_charger = Charger {
            id: charger_id,
            uid: charger.uid,
            password: charger.password,
            name: None,
            charger_pub: "".to_string(),
            management_private: "".to_string(),
            wg_charger_ip: "0.0.0.0/0".parse().unwrap(),
            wg_server_ip: "0.0.0.0/0".parse().unwrap(),
            psk: "".to_string(),
            webinterface_port: 0,
            firmware_version: "invalid".to_string(),
        };
        diesel::insert_into(c::chargers)
            .values(&test_charger)
            .execute(&mut conn)
            .unwrap();

        // Add allowed_user entry
        let allowed_user = AllowedUser {
            id: Uuid::new_v4(),
            user_id: user.id,
            charger_id,
            charger_uid: charger.uid,
            valid: true,
            note: None,
            name: None,
        };
        diesel::insert_into(au::allowed_users)
            .values(&allowed_user)
            .execute(&mut conn)
            .unwrap();

        let app = App::new()
            .configure(configure)
            .service(me)
            .wrap(crate::middleware::jwt::JwtMiddleware);
        let app = test::init_service(app).await;

        let (token, _) = verify_and_login_user(mail, key).await;
        let req = test::TestRequest::get()
            .uri("/me")
            .cookie(Cookie::new("access_token", token))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_success());

        let body: UserInfo = test::read_body_json(resp).await;
        assert!(!body.has_old_charger);
    }
}
