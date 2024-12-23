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

use actix_web::{error::ErrorBadRequest, put, web, HttpRequest, HttpResponse, Responder};
use base64::Engine;
use db_connector::models::{allowed_users::AllowedUser, wg_keys::WgKey};
use diesel::prelude::*;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::{
    error::Error,
    rate_limit::ChargerRateLimiter,
    routes::{
        auth::login::{validate_password, FindBy},
        charger::add::get_charger_from_db,
        user::get_user_id,
    },
    utils::{get_connection, parse_uuid, web_block_unpacked},
    AppState,
};

use super::add::{password_matches, Keys};

#[derive(Debug, Deserialize, Serialize, Clone, ToSchema)]
pub enum UserAuth {
    LoginKey(String),
}

#[derive(Debug, Deserialize, Serialize, ToSchema, Clone)]
pub struct AllowUserSchema {
    charger_id: String,
    charger_password: String,
    email: String,
    user_auth: UserAuth,
    wg_keys: [Keys; 5],
    #[schema(value_type = Vec<u32>)]
    charger_name: String,
    note: String,
}

async fn add_keys(
    state: &web::Data<AppState>,
    keys: [super::add::Keys; 5],
    uid: uuid::Uuid,
    cid: uuid::Uuid,
) -> actix_web::Result<()> {
    let mut conn = get_connection(&state)?;
    web_block_unpacked(move || {
        use db_connector::schema::wg_keys::dsl::*;

        let insert_keys: Vec<WgKey> = keys
            .into_iter()
            .map(|key| WgKey {
                id: uuid::Uuid::new_v4(),
                user_id: uid,
                charger_id: cid,
                in_use: false,
                charger_pub: key.charger_public,
                web_private: key.web_private,
                psk: key.psk,
                web_address: key.web_address,
                charger_address: key.charger_address,
                connection_no: key.connection_no as i32,
            })
            .collect();

        match diesel::insert_into(wg_keys)
            .values(&insert_keys)
            .execute(&mut conn)
        {
            Ok(_) => Ok(()),
            Err(_err) => Err(Error::InternalError),
        }
    })
    .await?;

    Ok(())
}

async fn authenticate_user(
    uid: uuid::Uuid,
    auth: &UserAuth,
    state: &web::Data<AppState>,
) -> actix_web::Result<()> {
    match auth {
        UserAuth::LoginKey(key) => {
            let conn = get_connection(state)?;
            let key = match base64::engine::general_purpose::STANDARD.decode(key) {
                Ok(v) => v,
                Err(_) => return Err(ErrorBadRequest("login_key is wrong base64")),
            };
            let _ = validate_password(&key, FindBy::Uuid(uid), conn).await?;
        }
    }
    Ok(())
}

/// Give another user permission to access a charger owned by the user.
#[utoipa::path(
    context_path = "/charger",
    request_body = AllowUserSchema,
    responses(
        (status = 200, description = "Allowing the user to access the charger was successful."),
        (status = 400, description = "The user does not exist.")
    )
)]
#[put("/allow_user")]
pub async fn allow_user(
    state: web::Data<AppState>,
    allow_user: web::Json<AllowUserSchema>,
    rate_limiter: web::Data<ChargerRateLimiter>,
    req: HttpRequest,
) -> Result<impl Responder, actix_web::Error> {
    rate_limiter.check(allow_user.charger_id.clone(), &req)?;

    let cid = parse_uuid(&allow_user.charger_id)?;

    let charger = get_charger_from_db(cid, &state).await?;

    if !password_matches(&allow_user.charger_password, &charger.password)? {
        return Err(Error::Unauthorized.into());
    }

    let allowed_uuid = get_user_id(&state, FindBy::Email(allow_user.email.clone())).await?;
    authenticate_user(allowed_uuid, &allow_user.user_auth, &state).await?;

    // delete old allowed_user when existing
    let mut conn = get_connection(&state)?;
    let allow_user = allow_user.clone();
    web_block_unpacked(move || {
        use db_connector::schema::allowed_users::dsl::*;

        match diesel::delete(
            allowed_users
                .filter(user_id.eq(allowed_uuid))
                .filter(charger_id.eq(cid)),
        )
        .execute(&mut conn)
        {
            Ok(_) => Ok(()),
            Err(_) => Err(Error::InternalError),
        }
    })
    .await?;

    // add new allowed_user
    let mut conn = get_connection(&state)?;
    let allow_user = allow_user.clone();
    web_block_unpacked(move || {
        use db_connector::schema::allowed_users::dsl::*;

        let u = AllowedUser {
            id: uuid::Uuid::new_v4(),
            user_id: allowed_uuid,
            charger_id: cid,
            charger_uid: charger.uid,
            valid: true,
            name: Some(allow_user.charger_name),
            note: Some(allow_user.note),
        };

        match diesel::insert_into(allowed_users)
            .values(u)
            .execute(&mut conn)
        {
            Ok(_) => Ok(()),
            Err(_err) => Err(Error::InternalError),
        }
    })
    .await?;

    match add_keys(&state, allow_user.wg_keys, allowed_uuid, cid).await {
        Ok(_) => (),
        Err(_err) => {}
    }

    Ok(HttpResponse::Ok())
}

#[cfg(test)]
pub mod tests {
    use std::str::FromStr;

    use super::*;
    use actix_web::{test, App};
    use base64::prelude::BASE64_STANDARD;
    use db_connector::test_connection_pool;
    use rand::{
        distributions::{Alphanumeric, DistString},
        RngCore,
    };
    use rand_core::OsRng;

    use crate::{
        routes::{
            charger::{add::tests::generate_random_keys, tests::TestCharger},
            user::tests::{get_test_uuid, TestUser},
        },
        tests::configure,
    };

    pub async fn add_allowed_test_user(email: &str, user_auth: UserAuth, charger: &TestCharger) {
        let app = App::new().configure(configure).service(allow_user);
        let app = test::init_service(app).await;

        let body = AllowUserSchema {
            charger_id: charger.uuid.to_string(),
            user_auth,
            email: email.to_string(),
            charger_password: charger.password.clone(),
            wg_keys: generate_random_keys(),
            charger_name: String::new(),
            note: String::new(),
        };
        let req = test::TestRequest::put()
            .uri("/allow_user")
            .append_header(("X-Forwarded-For", "123.123.123.3"))
            .set_json(body)
            .to_request();
        let resp = test::call_service(&app, req).await;
        println!("{}", resp.status());
        println!("{:?}", resp.response().body());
        assert!(resp.status().is_success());
    }

    #[actix_web::test]
    async fn test_allow_users() {
        let (user2, _) = TestUser::random().await;
        let (mut user1, _) = TestUser::random().await;

        let charger = OsRng.next_u32() as i32;
        user1.login().await.to_string();
        let charger = user1.add_charger(charger).await;

        let app = App::new().configure(configure).service(allow_user);
        let app = test::init_service(app).await;

        let allow = AllowUserSchema {
            charger_id: charger.uuid,
            user_auth: UserAuth::LoginKey(BASE64_STANDARD.encode(user2.get_login_key().await)),
            email: user2.mail.to_owned(),
            charger_password: charger.password,
            wg_keys: generate_random_keys(),
            charger_name: String::new(),
            note: String::new(),
        };
        let req = test::TestRequest::put()
            .uri("/allow_user")
            .append_header(("X-Forwarded-For", "123.123.123.3"))
            .set_json(allow)
            .to_request();
        let resp = test::call_service(&app, req).await;

        assert!(resp.status().is_success());
    }

    #[actix_web::test]
    async fn test_allow_wrong_credentials() {
        let (user2, _) = TestUser::random().await;
        let (mut user1, _) = TestUser::random().await;

        let charger = OsRng.next_u32() as i32;
        user1.login().await.to_string();
        let charger = user1.add_charger(charger).await;

        let app = App::new().configure(configure).service(allow_user);
        let app = test::init_service(app).await;

        let allow = AllowUserSchema {
            charger_id: charger.uuid,
            user_auth: UserAuth::LoginKey(BASE64_STANDARD.encode(user2.get_login_key().await)),
            email: user2.mail.to_owned(),
            charger_password: Alphanumeric.sample_string(&mut rand::thread_rng(), 32),
            wg_keys: generate_random_keys(),
            charger_name: String::new(),
            note: String::new(),
        };
        let req = test::TestRequest::put()
            .uri("/allow_user")
            .append_header(("X-Forwarded-For", "123.123.123.3"))
            .set_json(allow)
            .to_request();
        let resp = test::call_service(&app, req).await;

        assert_eq!(resp.status(), 401);
    }

    #[actix_web::test]
    async fn test_allow_users_non_existing() {
        let (mut user, _) = TestUser::random().await;

        let charger = OsRng.next_u32() as i32;
        user.login().await.to_string();
        let charger = user.add_charger(charger).await;

        let app = App::new().configure(configure).service(allow_user);
        let app = test::init_service(app).await;

        let allow = AllowUserSchema {
            charger_id: charger.uuid,
            user_auth: UserAuth::LoginKey(BASE64_STANDARD.encode(Vec::new())),
            email: uuid::Uuid::new_v4().to_string(),
            charger_password: String::new(),
            wg_keys: generate_random_keys(),
            charger_name: String::new(),
            note: String::new(),
        };
        let req = test::TestRequest::put()
            .uri("/allow_user")
            .append_header(("X-Forwarded-For", "123.123.123.3"))
            .set_json(allow)
            .to_request();
        let resp = test::call_service(&app, req).await;

        assert!(resp.status().is_client_error());
    }

    #[actix_web::test]
    async fn test_allow_already_allowed_user() {
        let (mut user, _) = TestUser::random().await;
        let (user2, _) = TestUser::random().await;
        user.login().await;
        let charger = user.add_random_charger().await;

        let app = App::new().configure(configure).service(allow_user);
        let app = test::init_service(app).await;

        let allow = AllowUserSchema {
            charger_id: charger.uuid.clone(),
            user_auth: UserAuth::LoginKey(BASE64_STANDARD.encode(user2.get_login_key().await)),
            email: user2.mail.to_owned(),
            charger_password: charger.password.clone(),
            wg_keys: generate_random_keys(),
            charger_name: String::new(),
            note: String::new(),
        };
        let req = test::TestRequest::put()
            .uri("/allow_user")
            .append_header(("X-Forwarded-For", "123.123.123.3"))
            .set_json(allow)
            .to_request();
        let resp = test::call_service(&app, req).await;

        assert!(resp.status().is_success());

        let allow = AllowUserSchema {
            charger_id: charger.uuid.clone(),
            user_auth: UserAuth::LoginKey(BASE64_STANDARD.encode(user2.get_login_key().await)),
            email: user2.mail.to_owned(),
            charger_password: charger.password.clone(),
            wg_keys: generate_random_keys(),
            charger_name: String::new(),
            note: String::new(),
        };
        let req = test::TestRequest::put()
            .uri("/allow_user")
            .append_header(("X-Forwarded-For", "123.123.123.3"))
            .set_json(allow)
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_success());

        let pool = test_connection_pool();
        let mut conn = pool.get().unwrap();
        {
            use db_connector::schema::allowed_users::dsl::*;

            let users: Vec<AllowedUser> = allowed_users
                .filter(user_id.eq(get_test_uuid(&user2.mail).unwrap()))
                .filter(charger_id.eq(uuid::Uuid::from_str(&charger.uuid).unwrap()))
                .select(AllowedUser::as_select())
                .load(&mut conn)
                .unwrap();

            assert_eq!(users.len(), 1);
        }
    }
}
