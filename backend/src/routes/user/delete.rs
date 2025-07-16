use actix_web::{delete, web, HttpResponse, Responder};
use db_connector::models::{allowed_users::AllowedUser, chargers::Charger};
use diesel::{prelude::*, result::Error::NotFound};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::{
    error::Error,
    routes::{
        auth::login::{validate_password, FindBy},
        charger::remove::{
            delete_all_allowed_users, delete_all_keys, delete_charger, remove_charger_from_state,
        },
        user::logout::delete_all_refresh_tokens,
    },
    utils::{get_connection, web_block_unpacked},
    AppState, BridgeState,
};

#[derive(ToSchema, Serialize, Deserialize)]
pub struct DeleteUserSchema {
    #[schema(value_type = Vec<u32>)]
    pub login_key: Vec<u8>,
}

async fn get_all_chargers_for_user(
    user_id: uuid::Uuid,
    state: &web::Data<AppState>,
) -> actix_web::Result<Vec<Charger>> {
    let mut conn = get_connection(state)?;
    let allowed_users: Vec<AllowedUser> = web_block_unpacked(move || {
        use db_connector::schema::allowed_users::dsl as allowed_users;

        match allowed_users::allowed_users
            .filter(allowed_users::user_id.eq(user_id))
            .select(AllowedUser::as_select())
            .load(&mut conn)
        {
            Ok(v) => Ok(v),
            Err(NotFound) => Ok(Vec::new()),
            Err(_err) => Err(Error::InternalError),
        }
    })
    .await?;

    let charger_ids: Vec<uuid::Uuid> = allowed_users.into_iter().map(|u| u.charger_id).collect();
    let mut conn = get_connection(state)?;
    web_block_unpacked(move || {
        use db_connector::schema::chargers::dsl::*;

        match chargers
            .filter(id.eq_any(charger_ids))
            .select(Charger::as_select())
            .load(&mut conn)
        {
            Ok(v) => Ok(v),
            Err(NotFound) => Ok(Vec::new()),
            Err(_err) => Err(Error::InternalError),
        }
    })
    .await
}

#[utoipa::path(
    context_path = "/user",
    request_body = DeleteUserSchema,
    responses(
        (status = 200),
        (status = 400, description = "Wrong password"),
        (status = 500)
    )
)]
#[delete("/delete")]
pub async fn delete_user(
    state: web::Data<AppState>,
    bridge_state: web::Data<BridgeState>,
    user_id: crate::models::uuid::Uuid,
    payload: web::Json<DeleteUserSchema>,
) -> actix_web::Result<impl Responder> {
    let user_id = user_id.into();

    let conn = get_connection(&state)?;
    let _ = validate_password(&payload.login_key, FindBy::Uuid(user_id), conn).await?;

    let chargers = get_all_chargers_for_user(user_id, &state).await?;
    let charger_ids: Vec<uuid::Uuid> = chargers.into_iter().map(|c| c.id).collect();
    for id in charger_ids.into_iter() {
        delete_all_keys(id, &state).await?;
        delete_all_allowed_users(id, &state).await?;
        delete_charger(id, &state).await?;
        remove_charger_from_state(id, &bridge_state).await;
    }

    delete_all_refresh_tokens(user_id, &state).await?;
    let mut conn = get_connection(&state)?;
    web_block_unpacked(move || {
        use db_connector::schema::users::dsl::*;

        match diesel::delete(users.find(user_id)).execute(&mut conn) {
            Ok(_) => Ok(()),
            Err(_err) => {
                println!("err: {_err:?}");
                Err(Error::InternalError)
            }
        }
    })
    .await?;

    Ok(HttpResponse::Ok())
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use actix_web::{cookie::Cookie, test, App};
    use db_connector::{
        models::{allowed_users::AllowedUser, chargers::Charger, users::User, wg_keys::WgKey},
        test_connection_pool,
    };
    use diesel::{prelude::*, result::Error::NotFound};

    use crate::{
        middleware::jwt::JwtMiddleware,
        routes::{
            auth::get_login_salt::tests::get_test_login_salt,
            user::tests::{get_test_uuid, hash_test_key, TestUser},
        },
        tests::configure,
        utils::generate_random_bytes,
    };

    use super::{delete_user, DeleteUserSchema};

    //TODO: add test for shared charger once it is merged
    #[actix_web::test]
    async fn test_delete() {
        let (mut user1, user1_mail) = TestUser::random().await;
        let (mut user2, user2_mail) = TestUser::random().await;
        let token = user1.login().await.to_owned();
        user2.login().await;
        let charger = user1.add_random_charger().await;
        let charger2 = user2.add_random_charger().await;
        let uid1 = get_test_uuid(&user1_mail).unwrap();
        let uid2 = get_test_uuid(&user2_mail).unwrap();

        let app = App::new()
            .configure(configure)
            .wrap(JwtMiddleware)
            .service(delete_user);
        let app = test::init_service(app).await;

        let login_salt = get_test_login_salt(&user1_mail).await;
        let login_key = hash_test_key(&user1.password, &login_salt, None);
        let schema = DeleteUserSchema { login_key };
        let req = test::TestRequest::delete()
            .uri("/delete")
            .cookie(Cookie::new("access_token", token))
            .set_json(schema)
            .to_request();

        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 200);

        let pool = test_connection_pool();
        let mut conn = pool.get().unwrap();

        {
            use db_connector::schema::allowed_users::dsl::*;

            let res = allowed_users
                .filter(user_id.eq(uid1))
                .select(AllowedUser::as_select())
                .get_result(&mut conn);
            assert_eq!(res, Err(NotFound));

            let res = allowed_users
                .filter(user_id.eq(uid2))
                .select(AllowedUser::as_select())
                .get_result(&mut conn);
            assert!(res.is_ok());
        }
        let uuid = uuid::Uuid::from_str(&charger.uuid).unwrap();
        let uuid2 = uuid::Uuid::from_str(&charger2.uuid).unwrap();
        {
            use db_connector::schema::chargers::dsl::*;

            let res = chargers
                .filter(id.eq(uuid))
                .select(Charger::as_select())
                .get_result(&mut conn);
            assert_eq!(res, Err(NotFound));

            let res = chargers
                .filter(id.eq(uuid2))
                .select(Charger::as_select())
                .get_result(&mut conn);
            assert!(res.is_ok());
        }
        {
            use db_connector::schema::wg_keys::dsl::*;

            let uuid = uuid::Uuid::from_str(&charger.uuid).unwrap();
            let res = wg_keys
                .filter(charger_id.eq(uuid))
                .select(WgKey::as_select())
                .get_result(&mut conn);
            assert_eq!(res, Err(NotFound));

            let res = wg_keys
                .filter(charger_id.eq(uuid2))
                .select(WgKey::as_select())
                .get_result(&mut conn);
            assert!(res.is_ok());
        }
        {
            use db_connector::schema::users::dsl::*;

            let res = users
                .find(uid1)
                .select(User::as_select())
                .get_result(&mut conn);
            assert_eq!(res, Err(NotFound));

            let res = users
                .find(uid2)
                .select(User::as_select())
                .get_result(&mut conn);
            assert!(res.is_ok());
        }
    }

    #[actix_web::test]
    async fn test_delete_wrong_password() {
        let (mut user1, user1_mail) = TestUser::random().await;
        let (mut user2, user2_mail) = TestUser::random().await;
        let token = user1.login().await.to_owned();
        user2.login().await;
        let charger = user1.add_random_charger().await;
        let charger2 = user2.add_random_charger().await;
        let uid1 = get_test_uuid(&user1_mail).unwrap();
        let uid2 = get_test_uuid(&user2_mail).unwrap();

        let app = App::new()
            .configure(configure)
            .wrap(JwtMiddleware)
            .service(delete_user);
        let app = test::init_service(app).await;

        let schema = DeleteUserSchema {
            login_key: generate_random_bytes(),
        };
        let req = test::TestRequest::delete()
            .uri("/delete")
            .cookie(Cookie::new("access_token", token))
            .set_json(schema)
            .to_request();

        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 401);

        let pool = test_connection_pool();
        let mut conn = pool.get().unwrap();

        {
            use db_connector::schema::allowed_users::dsl::*;

            let res = allowed_users
                .filter(user_id.eq(uid1))
                .select(AllowedUser::as_select())
                .get_result(&mut conn);
            assert!(res.is_ok());

            let res = allowed_users
                .filter(user_id.eq(uid2))
                .select(AllowedUser::as_select())
                .get_result(&mut conn);
            assert!(res.is_ok());
        }
        let uuid = uuid::Uuid::from_str(&charger.uuid).unwrap();
        let uuid2 = uuid::Uuid::from_str(&charger2.uuid).unwrap();
        {
            use db_connector::schema::chargers::dsl::*;

            let res = chargers
                .filter(id.eq(uuid))
                .select(Charger::as_select())
                .get_result(&mut conn);
            assert!(res.is_ok());

            let res = chargers
                .filter(id.eq(uuid2))
                .select(Charger::as_select())
                .get_result(&mut conn);
            assert!(res.is_ok());
        }
        {
            use db_connector::schema::wg_keys::dsl::*;

            let res = wg_keys
                .filter(charger_id.eq(uuid))
                .select(WgKey::as_select())
                .get_result(&mut conn);
            assert!(res.is_ok());

            let res = wg_keys
                .filter(charger_id.eq(uuid2))
                .select(WgKey::as_select())
                .get_result(&mut conn);
            assert!(res.is_ok());
        }
        {
            use db_connector::schema::users::dsl::*;

            let res = users
                .find(uid1)
                .select(User::as_select())
                .get_result(&mut conn);
            assert!(res.is_ok());

            let res = users
                .find(uid2)
                .select(User::as_select())
                .get_result(&mut conn);
            assert!(res.is_ok());
        }
    }
}
