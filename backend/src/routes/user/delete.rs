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
        charger::{
            get_devices::StateUpdateMessage,
            remove::{delete_charger, remove_charger_from_state},
        },
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
    let allowed_users: Vec<AllowedUser> = {
        let mut conn = get_connection(state).await?;
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
    let devices: Vec<Charger> = {
        let mut conn = get_connection(state).await?;
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
        (status = 401, description = "Access JWT is missing/invalid or the login key is incorrect"),
        (status = 500)
    ),
    security(
        ("jwt" = [])
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
        // Remove user's keys for this charger
        {
            let mut conn = get_connection(&state).await?;
            use db_connector::schema::wg_keys::dsl::*;
            diesel::delete(wg_keys.filter(user_id.eq(uid)).filter(charger_id.eq(cid)))
                .execute(&mut conn)
                .await
                .map_err(|_| Error::InternalError)?;
        }

        prompt_charger_to_remove_user(&bridge_state, cid, uid).await;

        // Check if any allowed users remain for this charger
        let allowed_count = {
            let mut conn = get_connection(&state).await?;
            use db_connector::schema::allowed_users::dsl::*;
            allowed_users
                .filter(charger_id.eq(cid))
                .count()
                .get_result::<i64>(&mut conn)
                .await
                .map_err(|_| Error::InternalError)?
        };
        if allowed_count == 0 {
            delete_charger(cid, &state).await?;
            remove_charger_from_state(cid, &bridge_state).await;
        }
        bridge_state
            .state_update_clients
            .send(
                uid,
                StateUpdateMessage::DeviceRemoved {
                    charger_id: cid.to_string(),
                },
            )
            .await;
    }

    delete_all_refresh_tokens(uid, &state).await?;
    {
        let mut conn = get_connection(&state).await?;
        use db_connector::schema::users::dsl::*;

        diesel::delete(users.find(uid))
            .execute(&mut conn)
            .await
            .map_err(|_| Error::InternalError)?;
    }

    // Deleting the account also cascades to its groupings, including for
    // accounts with no devices (and therefore no device removal messages).
    bridge_state
        .state_update_clients
        .send(uid, StateUpdateMessage::GroupingsChanged)
        .await;

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
        tests::{configure, create_test_bridge_state},
        utils::generate_random_bytes,
    };

    use super::{delete_user, DeleteUserSchema};

    #[actix_web::test]
    async fn deleting_account_without_devices_notifies_grouping_deletion() {
        let (mut user, _) = TestUser::random().await;
        user.login().await;
        let uid = get_test_uuid(&user.mail).await.unwrap();
        let pool = test_connection_pool();
        let mut conn = pool.get().await.unwrap();
        // Recovery and pending email verification records must not block
        // deletion of an otherwise authenticated account.
        {
            use db_connector::{
                models::{recovery_tokens::RecoveryToken, verification::Verification},
                schema::{recovery_tokens, verification},
            };
            diesel::insert_into(recovery_tokens::table)
                .values(RecoveryToken {
                    id: uuid::Uuid::new_v4(),
                    user_id: uid,
                    created: chrono::Utc::now().timestamp(),
                })
                .execute(&mut conn)
                .await
                .unwrap();
            diesel::insert_into(verification::table)
                .values(Verification::new(uid))
                .execute(&mut conn)
                .await
                .unwrap();
        }
        let grouping = crate::routes::grouping::test_helpers::create_test_grouping(
            user.get_access_token(),
            "group",
        )
        .await;
        let bridge = create_test_bridge_state(None);
        let (_, mut updates) = bridge
            .state_update_clients
            .subscribe(get_test_uuid(&user.mail).await.unwrap())
            .await;
        let app = test::init_service(
            App::new()
                .configure(configure)
                .app_data(bridge)
                .wrap(JwtMiddleware)
                .service(delete_user),
        )
        .await;
        let req = test::TestRequest::delete()
            .uri("/delete")
            .cookie(Cookie::new("access_token", user.get_access_token()))
            .set_json(DeleteUserSchema {
                login_key: user.get_login_key().await,
            })
            .to_request();
        assert_eq!(test::call_service(&app, req).await.status(), 200);
        assert!(matches!(
            updates.try_recv().unwrap(),
            crate::routes::charger::get_devices::StateUpdateMessage::GroupingsChanged
        ));
        assert!(updates.try_recv().is_err());
        assert!(
            crate::routes::grouping::test_helpers::get_grouping_from_db(&grouping.id)
                .await
                .is_none()
        );
        {
            use db_connector::schema::{recovery_tokens, verification};
            assert_eq!(
                recovery_tokens::table
                    .filter(recovery_tokens::user_id.eq(uid))
                    .count()
                    .get_result::<i64>(&mut conn)
                    .await
                    .unwrap(),
                0
            );
            assert_eq!(
                verification::table
                    .filter(verification::user.eq(uid))
                    .count()
                    .get_result::<i64>(&mut conn)
                    .await
                    .unwrap(),
                0
            );
        }
    }

    #[actix_web::test]
    async fn test_delete() {
        let (mut user1, user1_mail) = TestUser::random().await;
        let (mut user2, user2_mail) = TestUser::random().await;
        let token = user1.login().await.to_owned();
        user2.login().await;
        let device = user1.add_random_charger().await;
        let private_device = user1.add_random_charger().await;
        let device2 = user2.add_random_charger().await;
        // Share charger with user2
        let user2_auth = crate::routes::charger::allow_user::UserAuth::LoginKey(
            base64::prelude::BASE64_STANDARD.encode(&user2.get_login_key().await),
        );
        user1.allow_user(&user2_mail, user2_auth, &device).await;
        let uid1 = get_test_uuid(&user1_mail).await.unwrap();
        let uid2 = get_test_uuid(&user2_mail).await.unwrap();

        let bridge_state = create_test_bridge_state(None);
        let (_, mut updates) = bridge_state.state_update_clients.subscribe(uid1).await;
        let (_, mut second_tab) = bridge_state.state_update_clients.subscribe(uid1).await;
        let (_, mut other_user) = bridge_state.state_update_clients.subscribe(uid2).await;
        let app = App::new()
            .configure(configure)
            .app_data(bridge_state)
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
        for receiver in [&mut updates, &mut second_tab] {
            let mut removed = Vec::new();
            for _ in 0..2 {
                match receiver.try_recv().unwrap() {
                    crate::routes::charger::get_devices::StateUpdateMessage::DeviceRemoved {
                        charger_id,
                    } => removed.push(charger_id),
                    _ => panic!("Account deletion must remove its devices"),
                }
            }
            removed.sort();
            let mut expected = vec![device.uuid.clone(), private_device.uuid.clone()];
            expected.sort();
            assert_eq!(removed, expected);
            assert!(matches!(
                receiver.try_recv().unwrap(),
                crate::routes::charger::get_devices::StateUpdateMessage::GroupingsChanged
            ));
            assert!(receiver.try_recv().is_err());
        }
        assert!(other_user.try_recv().is_err());

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

        let bridge_state = create_test_bridge_state(None);
        let (_, mut updates) = bridge_state.state_update_clients.subscribe(uid1).await;
        let (_, mut second_tab) = bridge_state.state_update_clients.subscribe(uid1).await;
        let (_, mut other_user) = bridge_state.state_update_clients.subscribe(uid2).await;
        let app = App::new()
            .configure(configure)
            .app_data(bridge_state)
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
        assert!(updates.try_recv().is_err());
        assert!(second_tab.try_recv().is_err());
        assert!(other_user.try_recv().is_err());

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
