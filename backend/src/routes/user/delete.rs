use actix_web::{delete, web, HttpResponse, Responder};
use db_connector::models::{allowed_users::AllowedUser, chargers::Charger};
use diesel::{result::Error::NotFound, ExpressionMethods, QueryDsl, SelectableHelper};
use diesel_async::RunQueryDsl as _;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::{
    error::Error,
    routes::{
        auth::login::{validate_password, FindBy},
        charger::remove::{delete_charger, remove_charger_from_state},
        user::logout::delete_all_refresh_tokens,
    },
    udp_server::management::prompt_charger_to_remove_user,
    utils::get_connection,
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
    let mut conn = get_connection(state).await?;
    let allowed_users: Vec<AllowedUser> = {
        use db_connector::schema::allowed_users::dsl as allowed_users;

        match allowed_users::allowed_users
            .filter(allowed_users::user_id.eq(user_id))
            .select(AllowedUser::as_select())
            .load::<AllowedUser>(&mut conn)
            .await
        {
            Ok(v) => v,
            Err(NotFound) => Vec::new(),
            Err(_err) => return Err(Error::InternalError.into()),
        }
    };

    let device_ids: Vec<uuid::Uuid> = allowed_users.into_iter().map(|u| u.charger_id).collect();
    let mut conn = get_connection(state).await?;
    let devices: Vec<Charger> = {
        use db_connector::schema::chargers::dsl::*;

        match chargers
            .filter(id.eq_any(device_ids))
            .select(Charger::as_select())
            .load::<Charger>(&mut conn)
            .await
        {
            Ok(v) => v,
            Err(NotFound) => Vec::new(),
            Err(_err) => return Err(Error::InternalError.into()),
        }
    };

    Ok(devices)
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
    bridge_state: web::Data<BridgeState<'_>>,
    user_id: crate::models::uuid::Uuid,
    payload: web::Json<DeleteUserSchema>,
) -> actix_web::Result<impl Responder> {
    let uid = user_id.into();

    let _ = validate_password(&payload.login_key, FindBy::Uuid(uid), &state).await?;

    let devices = get_all_chargers_for_user(uid, &state).await?;
    let device_ids: Vec<uuid::Uuid> = devices.iter().map(|c| c.id).collect();
    for cid in device_ids.into_iter() {
        // Remove user from allowed_users for this charger
        {
            let mut conn = get_connection(&state).await?;
            {
                use db_connector::schema::allowed_users::dsl::*;
                diesel::delete(
                    allowed_users
                        .filter(user_id.eq(uid))
                        .filter(charger_id.eq(cid)),
                )
                .execute(&mut conn)
                .await
                .map_err(|_| Error::InternalError)?;
            }
        }
        // Remove user's keys for this charger
        {
            let mut conn = get_connection(&state).await?;
            {
                use db_connector::schema::wg_keys::dsl::*;
                diesel::delete(wg_keys.filter(user_id.eq(uid)).filter(charger_id.eq(cid)))
                    .execute(&mut conn)
                    .await
                    .map_err(|_| Error::InternalError)?;
            }
        }

        prompt_charger_to_remove_user(&bridge_state, cid, uid).await;

        // Check if any allowed users remain for this charger
        let allowed_count = {
            let mut conn = get_connection(&state).await?;
            {
                use db_connector::schema::allowed_users::dsl::*;
                allowed_users
                    .filter(charger_id.eq(cid))
                    .count()
                    .get_result::<i64>(&mut conn)
                    .await
                    .map_err(|_| Error::InternalError)?
            }
        };
        if allowed_count == 0 {
            delete_charger(cid, &state).await?;
            remove_charger_from_state(cid, &bridge_state).await;
        }
    }

    delete_all_refresh_tokens(uid, &state).await?;
    let mut conn = get_connection(&state).await?;
    {
        use db_connector::schema::users::dsl::*;

        diesel::delete(users.find(uid))
            .execute(&mut conn)
            .await
            .map_err(|e| {
                println!("err: {e:?}");
                Error::InternalError
            })?;
    }

    Ok(HttpResponse::Ok())
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use actix_web::{cookie::Cookie, test, App};
    use base64::Engine;
    use db_connector::{
        models::{allowed_users::AllowedUser, chargers::Charger, users::User, wg_keys::WgKey},
        test_connection_pool,
    };
    use diesel::{
        result::Error::NotFound, BoolExpressionMethods, ExpressionMethods, QueryDsl,
        SelectableHelper,
    };
    use diesel_async::RunQueryDsl as _AsyncRunQueryDsl;

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

    #[actix_web::test]
    async fn test_delete() {
        let (mut user1, user1_mail) = TestUser::random().await;
        let (mut user2, user2_mail) = TestUser::random().await;
        let token = user1.login().await.to_owned();
        user2.login().await;
        let device = user1.add_random_charger().await;
        let device2 = user2.add_random_charger().await;
        // Share charger with user2
        let user2_auth = crate::routes::charger::allow_user::UserAuth::LoginKey(
            base64::prelude::BASE64_STANDARD.encode(&user2.get_login_key().await),
        );
        user1.allow_user(&user2_mail, user2_auth, &device).await;
        let uid1 = get_test_uuid(&user1_mail).await.unwrap();
        let uid2 = get_test_uuid(&user2_mail).await.unwrap();

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
        let mut conn = pool.get().await.unwrap();

        {
            use db_connector::schema::allowed_users::dsl::*;

            // user1 should be gone
            let res = allowed_users
                .filter(user_id.eq(uid1))
                .select(AllowedUser::as_select())
                .get_result::<AllowedUser>(&mut conn)
                .await;
            assert_eq!(res, Err(NotFound));

            // user2 should still have access to both chargers
            let res: Vec<AllowedUser> = allowed_users
                .filter(user_id.eq(uid2))
                .select(AllowedUser::as_select())
                .load::<AllowedUser>(&mut conn)
                .await
                .expect("load allowed users");
            let device_ids: Vec<uuid::Uuid> = res.into_iter().map(|au| au.charger_id).collect();
            let uuid = uuid::Uuid::from_str(&device.uuid).unwrap();
            let uuid2 = uuid::Uuid::from_str(&device2.uuid).unwrap();
            println!("device_ids: {device_ids:?}");
            assert!(device_ids.contains(&uuid));
            assert!(device_ids.contains(&uuid2));
        }
        let uuid = uuid::Uuid::from_str(&device.uuid).unwrap();
        let uuid2 = uuid::Uuid::from_str(&device2.uuid).unwrap();
        {
            use db_connector::schema::chargers::dsl::*;

            // Both chargers should still exist
            let res = chargers
                .filter(id.eq(uuid))
                .select(Charger::as_select())
                .get_result::<Charger>(&mut conn)
                .await;
            assert!(res.is_ok());

            let res = chargers
                .filter(id.eq(uuid2))
                .select(Charger::as_select())
                .get_result::<Charger>(&mut conn)
                .await;
            assert!(res.is_ok());
        }
        {
            use db_connector::schema::wg_keys::dsl::*;

            // Only user2's keys should remain for both chargers
            let uuid_inner = uuid::Uuid::from_str(&device.uuid).unwrap();
            let uuid2_inner = uuid::Uuid::from_str(&device2.uuid).unwrap();
            let keys_user2: Vec<WgKey> = wg_keys
                .filter(charger_id.eq(uuid_inner).or(charger_id.eq(uuid2_inner)))
                .select(WgKey::as_select())
                .load::<WgKey>(&mut conn)
                .await
                .unwrap()
                .into_iter()
                .filter(|k| k.user_id == uid2)
                .collect();
            assert!(!keys_user2.is_empty());
        }
        {
            use db_connector::schema::users::dsl::*;

            // user1 should be deleted, user2 should remain
            let res = users
                .find(uid1)
                .select(User::as_select())
                .get_result::<User>(&mut conn)
                .await;
            assert_eq!(res, Err(NotFound));

            let res = users
                .find(uid2)
                .select(User::as_select())
                .get_result::<User>(&mut conn)
                .await;
            assert!(res.is_ok());
        }
    }

    #[actix_web::test]
    async fn test_delete_wrong_password() {
        let (mut user1, user1_mail) = TestUser::random().await;
        let (mut user2, user2_mail) = TestUser::random().await;
        let token = user1.login().await.to_owned();
        user2.login().await;
        let device = user1.add_random_charger().await;
        let device2 = user2.add_random_charger().await;
        let uid1 = get_test_uuid(&user1_mail).await.unwrap();
        let uid2 = get_test_uuid(&user2_mail).await.unwrap();

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
        let mut conn = pool.get().await.unwrap();

        {
            use db_connector::schema::allowed_users::dsl::*;

            let res = allowed_users
                .filter(user_id.eq(uid1))
                .select(AllowedUser::as_select())
                .get_result::<AllowedUser>(&mut conn)
                .await;
            assert!(res.is_ok());

            let res = allowed_users
                .filter(user_id.eq(uid2))
                .select(AllowedUser::as_select())
                .get_result::<AllowedUser>(&mut conn)
                .await;
            assert!(res.is_ok());
        }
        let uuid = uuid::Uuid::from_str(&device.uuid).unwrap();
        let uuid2 = uuid::Uuid::from_str(&device2.uuid).unwrap();
        {
            use db_connector::schema::chargers::dsl::*;

            let res = chargers
                .filter(id.eq(uuid))
                .select(Charger::as_select())
                .get_result::<Charger>(&mut conn)
                .await;
            assert!(res.is_ok());

            let res = chargers
                .filter(id.eq(uuid2))
                .select(Charger::as_select())
                .get_result::<Charger>(&mut conn)
                .await;
            assert!(res.is_ok());
        }
        {
            use db_connector::schema::wg_keys::dsl::*;

            let res = wg_keys
                .filter(charger_id.eq(uuid))
                .select(WgKey::as_select())
                .get_result::<WgKey>(&mut conn)
                .await;
            assert!(res.is_ok());

            let res = wg_keys
                .filter(charger_id.eq(uuid2))
                .select(WgKey::as_select())
                .get_result::<WgKey>(&mut conn)
                .await;
            assert!(res.is_ok());
        }
        {
            use db_connector::schema::users::dsl::*;

            let res = users
                .find(uid1)
                .select(User::as_select())
                .get_result::<User>(&mut conn)
                .await;
            assert!(res.is_ok());

            let res = users
                .find(uid2)
                .select(User::as_select())
                .get_result::<User>(&mut conn)
                .await;
            assert!(res.is_ok());
        }
    }
}
