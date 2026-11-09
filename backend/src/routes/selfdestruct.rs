use actix_web::{delete, web, HttpRequest, HttpResponse, Responder};
use db_connector::models::chargers::Charger;
use diesel::{ExpressionMethods, QueryDsl};
use diesel_async::RunQueryDsl as _;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::{
    error::Error,
    rate_limit::ChargerRateLimiter,
    routes::charger::{
        add::password_matches, get_devices::StateUpdateMessage, remove::remove_charger_from_state,
    },
    utils::{get_charger_by_uid, get_charger_from_db, get_connection, parse_uuid},
    AppState, BridgeState,
};

#[derive(ToSchema, Serialize, Deserialize)]
pub struct SelfdestructSchema {
    pub id: Option<i32>,
    pub uuid: Option<String>,
    pub password: String,
}

async fn get_charger(
    schema: SelfdestructSchema,
    state: &web::Data<AppState>,
    rate_limiter: &web::Data<ChargerRateLimiter>,
    req: &HttpRequest,
) -> actix_web::Result<Charger> {
    if let Some(uuid) = schema.uuid {
        rate_limiter.check(uuid.clone(), req)?;
        let device_id = parse_uuid(&uuid)?;
        let device = get_charger_from_db(device_id, state).await?;
        if !password_matches(&schema.password, &device.password, &state.hasher).await? {
            return Err(Error::ChargerCredentialsWrong.into());
        }
        Ok(device)
    } else if let Some(uid) = schema.id {
        rate_limiter.check(uid.to_string(), req)?;
        Ok(get_charger_by_uid(uid, Some(schema.password), state).await?)
    } else {
        Err(Error::ChargerCredentialsWrong.into())
    }
}

// Chargers can delete themselves via this route
#[utoipa::path(
    request_body = SelfdestructSchema,
    responses(
        (status = 200, description = "Everything worked fine and the charger was deleted")
    )
)]
#[delete("/selfdestruct")]
pub async fn selfdestruct(
    payload: web::Json<SelfdestructSchema>,
    state: web::Data<AppState>,
    bridge_state: web::Data<BridgeState<'_>>,
    rate_limiter: web::Data<ChargerRateLimiter>,
    req: HttpRequest,
) -> actix_web::Result<impl Responder> {
    // funtion does also the rate limiting
    let device = get_charger(payload.0, &state, &rate_limiter, &req).await?;

    let removed_users: Vec<uuid::Uuid> = {
        let mut conn = get_connection(&state).await?;
        use db_connector::schema::allowed_users::dsl as allowed_users;

        match diesel::delete(
            allowed_users::allowed_users.filter(allowed_users::charger_id.eq(device.id)),
        )
        .returning(allowed_users::user_id)
        .get_results(&mut conn)
        .await
        {
            Ok(users) => users,
            Err(_err) => return Err(Error::InternalError.into()),
        }
    };

    {
        let mut conn = get_connection(&state).await?;
        use db_connector::schema::wg_keys::dsl as wg_keys;

        match diesel::delete(wg_keys::wg_keys.filter(wg_keys::charger_id.eq(device.id)))
            .execute(&mut conn)
            .await
        {
            Ok(_) => {}
            Err(_err) => return Err(Error::InternalError.into()),
        }
    }

    {
        let mut conn = get_connection(&state).await?;
        use db_connector::schema::chargers::dsl::*;

        match diesel::delete(chargers.filter(id.eq(device.id)))
            .execute(&mut conn)
            .await
        {
            Ok(_) => {}
            Err(_err) => return Err(Error::InternalError.into()),
        }
    }

    remove_charger_from_state(device.id, &bridge_state).await;
    for user_id in removed_users {
        bridge_state
            .state_update_clients
            .send(
                user_id,
                StateUpdateMessage::DeviceRemoved {
                    charger_id: device.id.to_string(),
                },
            )
            .await;
    }

    Ok(HttpResponse::Ok().body(()))
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use actix_web::{
        test::{self, TestRequest},
        App,
    };
    use db_connector::{
        models::{allowed_users::AllowedUser, chargers::Charger, wg_keys::WgKey},
        test_connection_pool,
    };
    use diesel::{ExpressionMethods, QueryDsl, SelectableHelper};
    use diesel_async::RunQueryDsl as _AsyncRunQueryDsl;

    use base64::{prelude::BASE64_STANDARD, Engine};

    use crate::{
        routes::{
            charger::{allow_user::UserAuth, get_devices::StateUpdateMessage},
            user::tests::{get_test_uuid, TestUser},
        },
        tests::{configure, create_test_bridge_state},
    };

    use super::{selfdestruct, SelfdestructSchema};

    async fn check_selfdestruct_notifications(legacy_id: bool) {
        let (mut owner, _) = TestUser::random().await;
        let (mut guest, _) = TestUser::random().await;
        owner.login().await;
        guest.login().await;
        let device = owner.add_random_charger().await;
        owner
            .allow_user(
                &guest.mail,
                UserAuth::LoginKey(BASE64_STANDARD.encode(guest.get_login_key().await)),
                &device,
            )
            .await;

        let bridge_state = create_test_bridge_state(None);
        let owner_id = get_test_uuid(&owner.mail).await.unwrap();
        let guest_id = get_test_uuid(&guest.mail).await.unwrap();
        let (_, mut owner_tab) = bridge_state.state_update_clients.subscribe(owner_id).await;
        let (_, mut owner_second_tab) = bridge_state.state_update_clients.subscribe(owner_id).await;
        let (_, mut guest_tab) = bridge_state.state_update_clients.subscribe(guest_id).await;
        let (_, mut unrelated_tab) = bridge_state
            .state_update_clients
            .subscribe(uuid::Uuid::new_v4())
            .await;
        let app = test::init_service(
            App::new()
                .configure(configure)
                .app_data(bridge_state)
                .service(selfdestruct),
        )
        .await;

        let mut schema = SelfdestructSchema {
            id: legacy_id.then_some(device.uid),
            uuid: if legacy_id {
                None
            } else {
                Some(device.uuid.clone())
            },
            password: "incorrect-password".into(),
        };
        let req = TestRequest::delete()
            .uri("/selfdestruct")
            .append_header(("X-Forwarded-For", "123.123.123.3"))
            .set_json(&schema)
            .to_request();
        assert!(test::call_service(&app, req)
            .await
            .status()
            .is_client_error());
        for receiver in [
            &mut owner_tab,
            &mut owner_second_tab,
            &mut guest_tab,
            &mut unrelated_tab,
        ] {
            assert!(receiver.try_recv().is_err());
        }

        schema.password = device.password;
        let req = TestRequest::delete()
            .uri("/selfdestruct")
            .append_header(("X-Forwarded-For", "123.123.123.3"))
            .set_json(&schema)
            .to_request();
        assert_eq!(test::call_service(&app, req).await.status(), 200);
        for receiver in [&mut owner_tab, &mut owner_second_tab, &mut guest_tab] {
            assert!(matches!(receiver.try_recv().unwrap(),
                StateUpdateMessage::DeviceRemoved { charger_id } if charger_id == device.uuid));
            assert!(receiver.try_recv().is_err());
        }
        assert!(unrelated_tab.try_recv().is_err());
    }

    #[actix_web::test]
    async fn selfdestruct_notifies_all_users_and_tabs() {
        check_selfdestruct_notifications(false).await;
    }

    #[actix_web::test]
    async fn legacy_selfdestruct_notifies_all_users_and_tabs() {
        check_selfdestruct_notifications(true).await;
    }

    #[actix_web::test]
    async fn test_selfdestruction() {
        let (mut user, _) = TestUser::random().await;
        user.login().await;
        let device = user.add_random_charger().await;

        let app = App::new().configure(configure).service(selfdestruct);
        let app = test::init_service(app).await;

        let schema = SelfdestructSchema {
            id: None,
            uuid: Some(device.uuid.clone()),
            password: device.password,
        };

        let req = TestRequest::delete()
            .uri("/selfdestruct")
            .append_header(("X-Forwarded-For", "123.123.123.3"))
            .set_json(schema)
            .to_request();

        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 200);
        let pool = test_connection_pool();
        let mut conn = pool.get().await.unwrap();

        let cid = uuid::Uuid::from_str(&device.uuid).unwrap();
        let devices: Vec<Charger> = {
            use db_connector::schema::chargers::dsl as chargers;

            chargers::chargers
                .filter(chargers::id.eq(cid))
                .select(Charger::as_select())
                .load::<Charger>(&mut conn)
                .await
                .unwrap()
        };
        assert_eq!(devices.len(), 0);

        let allowed_users: Vec<AllowedUser> = {
            use db_connector::schema::allowed_users::dsl::*;

            allowed_users
                .filter(charger_id.eq(cid))
                .select(AllowedUser::as_select())
                .load::<AllowedUser>(&mut conn)
                .await
                .unwrap()
        };
        assert_eq!(allowed_users.len(), 0);

        let wg_keys: Vec<WgKey> = {
            use db_connector::schema::wg_keys::dsl::*;

            wg_keys
                .filter(charger_id.eq(cid))
                .select(WgKey::as_select())
                .load::<WgKey>(&mut conn)
                .await
                .unwrap()
        };
        assert_eq!(wg_keys.len(), 0);
    }

    #[actix_web::test]
    async fn test_selfdestruction_depricated() {
        let (mut user, _) = TestUser::random().await;
        user.login().await;
        let device = user.add_random_charger().await;

        let app = App::new().configure(configure).service(selfdestruct);
        let app = test::init_service(app).await;

        let schema = SelfdestructSchema {
            id: Some(device.uid),
            uuid: None,
            password: device.password,
        };

        let req = TestRequest::delete()
            .uri("/selfdestruct")
            .append_header(("X-Forwarded-For", "123.123.123.3"))
            .set_json(schema)
            .to_request();

        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 200);
        let pool = test_connection_pool();
        let mut conn = pool.get().await.unwrap();

        let cid = uuid::Uuid::from_str(&device.uuid).unwrap();
        let devices: Vec<Charger> = {
            use db_connector::schema::chargers::dsl as chargers;

            chargers::chargers
                .filter(chargers::id.eq(cid))
                .select(Charger::as_select())
                .load::<Charger>(&mut conn)
                .await
                .unwrap()
        };
        assert_eq!(devices.len(), 0);

        let allowed_users: Vec<AllowedUser> = {
            use db_connector::schema::allowed_users::dsl::*;

            allowed_users
                .filter(charger_id.eq(cid))
                .select(AllowedUser::as_select())
                .load::<AllowedUser>(&mut conn)
                .await
                .unwrap()
        };
        assert_eq!(allowed_users.len(), 0);

        let wg_keys: Vec<WgKey> = {
            use db_connector::schema::wg_keys::dsl::*;

            wg_keys
                .filter(charger_id.eq(cid))
                .select(WgKey::as_select())
                .load::<WgKey>(&mut conn)
                .await
                .unwrap()
        };
        assert_eq!(wg_keys.len(), 0);
    }
}
