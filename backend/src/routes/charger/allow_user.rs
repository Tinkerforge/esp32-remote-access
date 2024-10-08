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

use actix_web::{put, web, HttpResponse, Responder};
use db_connector::models::{allowed_users::AllowedUser, wg_keys::WgKey};
use diesel::prelude::*;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::{
    error::Error,
    routes::{auth::login::{validate_password, FindBy}, charger::add::get_charger_from_db, user::get_user_id},
    utils::{get_connection, parse_uuid, web_block_unpacked},
    AppState,
};

use super::add::password_matches;

#[derive(Debug, Deserialize, Serialize, Clone)]
pub enum UserAuth {
    LoginKey(Vec<u8>)
}

#[derive(Debug, Deserialize, Serialize, ToSchema, Clone)]
pub struct AllowUserSchema {
    charger_id: String,
    pass: String,
    email: String,
    user_auth: UserAuth,
    wg_keys: [super::add::Keys; 5],
    key: Vec<u8>,
    charger_name: Vec<u8>,
    note: Vec<u8>,
}

async fn add_keys(state: &web::Data<AppState>, keys: [super::add::Keys; 5], uid: uuid::Uuid, cid: uuid::Uuid) -> actix_web::Result<()> {
    let mut conn = get_connection(&state)?;
    web_block_unpacked(move || {
        use db_connector::schema::wg_keys::dsl::*;

        let insert_keys: Vec<WgKey> = keys.into_iter().map(|key| {
            WgKey {
                id: uuid::Uuid::new_v4(),
                user_id: uid,
                charger_id: cid,
                in_use: false,
                charger_pub: key.charger_public,
                web_private: key.web_private,
                psk: key.psk,
                web_address: key.web_address,
                charger_address: key.charger_address,
                connection_no: key.connection_no as i32
            }
        }).collect();

        match diesel::insert_into(wg_keys).values(&insert_keys).execute(&mut conn) {
            Ok(_) => Ok(()),
            Err(_err) => Err(Error::InternalError)
        }
    }).await?;

    Ok(())
}

async fn authenticate_user(uid: uuid::Uuid, auth: &UserAuth, state: &web::Data<AppState>) -> actix_web::Result<()> {
    match auth {
        UserAuth::LoginKey(key) => {
            let conn = get_connection(state)?;
            let _ = validate_password(key, FindBy::Uuid(uid), conn).await?;
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
) -> Result<impl Responder, actix_web::Error> {
    let cid = parse_uuid(&allow_user.charger_id)?;

    let charger = get_charger_from_db(cid, &state).await?;

    if !password_matches(&allow_user.pass, &charger.password)? {
        return Err(Error::Unauthorized.into())
    }

    let allowed_uuid = get_user_id(&state, FindBy::Email(allow_user.email.clone())).await?;
    authenticate_user(allowed_uuid, &allow_user.user_auth, &state).await?;

    let mut conn = get_connection(&state)?;
    let allow_user = allow_user.clone();
    web_block_unpacked(move || {
        use db_connector::schema::allowed_users::dsl::*;

        let u = AllowedUser {
            id: uuid::Uuid::new_v4(),
            user_id: allowed_uuid,
            charger_id: cid,
            charger_uid: charger.uid,
            valid: false,
            key: Some(allow_user.key),
            name: Some(allow_user.charger_name),
            note: Some(allow_user.note)
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
        Err(_err) => {

        }
    }

    Ok(HttpResponse::Ok())
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use actix_web::{test, App};
    use rand::{distributions::{Alphanumeric, DistString}, RngCore};
    use rand_core::OsRng;

    use crate::{routes::{charger::{add::tests::generate_random_keys, tests::TestCharger}, user::tests::TestUser}, tests::configure};

    pub async fn add_allowed_test_user(email: &str, user_auth: UserAuth, charger: &TestCharger) {

        let app = App::new()
            .configure(configure)
            .service(allow_user);
        let app = test::init_service(app).await;

        let body = AllowUserSchema {
            charger_id: charger.uuid.to_string(),
            user_auth,
            email: email.to_string(),
            pass: charger.password.clone(),
            wg_keys: generate_random_keys(),
            key: Vec::new(),
            charger_name: Vec::new(),
            note: Vec::new(),
        };
        let req = test::TestRequest::put()
            .uri("/allow_user")
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

        let app = App::new()
            .configure(configure)
            .service(allow_user);
        let app = test::init_service(app).await;

        let allow = AllowUserSchema {
            charger_id: charger.uuid,
            user_auth: UserAuth::LoginKey(user2.get_login_key().await),
            email: user2.mail.to_owned(),
            pass: charger.password,
            wg_keys: generate_random_keys(),
            key: Vec::new(),
            charger_name: Vec::new(),
            note: Vec::new(),
        };
        let req = test::TestRequest::put()
            .uri("/allow_user")
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

        let app = App::new()
            .configure(configure)
            .service(allow_user);
        let app = test::init_service(app).await;

        let allow = AllowUserSchema {
            charger_id: charger.uuid,
            user_auth: UserAuth::LoginKey(user2.get_login_key().await),
            email: user2.mail.to_owned(),
            pass: Alphanumeric.sample_string(&mut rand::thread_rng(), 32),
            wg_keys: generate_random_keys(),
            key: Vec::new(),
            charger_name: Vec::new(),
            note: Vec::new(),
        };
        let req = test::TestRequest::put()
            .uri("/allow_user")
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

        let app = App::new()
            .configure(configure)
            .service(allow_user);
        let app = test::init_service(app).await;

        let allow = AllowUserSchema {
            charger_id: charger.uuid,
            user_auth: UserAuth::LoginKey(Vec::new()),
            email: uuid::Uuid::new_v4().to_string(),
            pass: String::new(),
            wg_keys: generate_random_keys(),
            key: Vec::new(),
            charger_name: Vec::new(),
            note: Vec::new(),
        };
        let req = test::TestRequest::put()
            .uri("/allow_user")
            .set_json(allow)
            .to_request();
        let resp = test::call_service(&app, req).await;

        assert!(resp.status().is_client_error());
    }
}
