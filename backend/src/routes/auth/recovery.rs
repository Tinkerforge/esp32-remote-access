use std::str::FromStr;

use actix_web::{error::ErrorBadRequest, post, web, HttpResponse, Responder};
use db_connector::models::recovery_tokens::RecoveryToken;
use diesel::{prelude::*, result::Error::NotFound};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::{
    error::Error,
    routes::auth::register::hash_key,
    utils::{get_connection, web_block_unpacked},
    AppState,
};

#[derive(Deserialize, Serialize, ToSchema)]
pub struct RecoverySchema {
    recovery_key: String,
    #[schema(value_type = Vec<u32>)]
    new_login_key: Vec<u8>,
    #[schema(value_type = Vec<u32>)]
    new_login_salt: Vec<u8>,
    #[schema(value_type = Vec<u32>)]
    new_secret_nonce: Vec<u8>,
    #[schema(value_type = Vec<u32>)]
    new_secret_salt: Vec<u8>,
    #[schema(value_type = Vec<u32>)]
    new_encrypted_secret: Vec<u8>,
    reused_secret: bool,
}

async fn get_user_id(state: &web::Data<AppState>, recovery_key: Uuid) -> actix_web::Result<Uuid> {
    let mut conn = get_connection(state)?;
    let token: RecoveryToken = web_block_unpacked(move || {
        use db_connector::schema::recovery_tokens::dsl::*;

        match recovery_tokens
            .filter(id.eq(recovery_key))
            .select(RecoveryToken::as_select())
            .get_result(&mut conn)
        {
            Ok(t) => Ok(t),
            Err(NotFound) => Err(Error::Unauthorized),
            Err(_err) => Err(Error::InternalError),
        }
    })
    .await?;

    let mut conn = get_connection(state)?;
    web_block_unpacked(move || {
        use db_connector::schema::recovery_tokens::dsl::*;

        match diesel::delete(recovery_tokens.find(recovery_key)).execute(&mut conn) {
            Ok(_) => Ok(()),
            Err(_err) => return Err(Error::InternalError),
        }
    })
    .await?;

    Ok(token.user_id)
}

async fn invalidate_wg_keys(state: &web::Data<AppState>, uid: Uuid) -> actix_web::Result<()> {
    let mut conn = get_connection(state)?;
    web_block_unpacked(move || {
        use db_connector::schema::wg_keys::dsl::*;

        match diesel::delete(wg_keys.filter(user_id.eq(uid))).execute(&mut conn) {
            Ok(_) => Ok(()),
            Err(_err) => Err(Error::InternalError),
        }
    })
    .await
}

async fn invalidate_chargers(state: &web::Data<AppState>, uid: Uuid) -> actix_web::Result<()> {
    let mut conn = get_connection(state)?;
    web_block_unpacked(move || {
        use db_connector::schema::allowed_users::dsl::*;

        match diesel::update(allowed_users.filter(user_id.eq(uid)))
            .set(valid.eq(false))
            .execute(&mut conn)
        {
            Ok(_) => Ok(()),
            Err(_err) => Err(Error::InternalError),
        }
    })
    .await
}

// Recover an account
#[utoipa::path(
    context_path = "/auth",
    responses(
        (status = 200, description = "Recovery was successful"),
        (status = 400, description = "Request contained invalid data")
    )
)]
#[post("/recovery")]
pub async fn recovery(
    data: web::Json<RecoverySchema>,
    state: web::Data<AppState>,
) -> actix_web::Result<impl Responder> {
    let recovery_key = match Uuid::from_str(&data.recovery_key) {
        Ok(uuid) => uuid,
        Err(_err) => {
            return Err(ErrorBadRequest("Recovery key has wrong format"));
        }
    };

    let user_id = get_user_id(&state, recovery_key).await?;

    if !data.reused_secret {
        invalidate_wg_keys(&state, user_id).await?;
        invalidate_chargers(&state, user_id).await?;
    }

    let mut conn = get_connection(&state)?;
    web_block_unpacked(move || {
        use db_connector::schema::users::dsl::*;

        let new_hash = match hash_key(&data.new_login_key) {
            Ok(hash) => hash,
            Err(_err) => return Err(Error::InternalError.into()),
        };

        match diesel::update(users.find(user_id))
            .set((
                login_key.eq(new_hash),
                login_salt.eq(&data.new_login_salt),
                secret.eq(&data.new_encrypted_secret),
                secret_nonce.eq(&data.new_secret_nonce),
                secret_salt.eq(&data.new_secret_salt),
            ))
            .execute(&mut conn)
        {
            Ok(_) => Ok(()),
            Err(_err) => Err(Error::InternalError),
        }
    })
    .await?;

    Ok(HttpResponse::Ok())
}

#[cfg(test)]
mod tests {
    use actix_web::{
        http::header::ContentType,
        test::{self, TestRequest},
        App,
    };
    use db_connector::{
        models::{allowed_users::AllowedUser, wg_keys::WgKey},
        test_connection_pool,
    };
    use diesel::prelude::*;
    use libsodium_sys::{
        crypto_box_SECRETKEYBYTES, crypto_secretbox_KEYBYTES, crypto_secretbox_MACBYTES,
        crypto_secretbox_NONCEBYTES, crypto_secretbox_easy, crypto_secretbox_open_easy,
    };

    use crate::{
        routes::{
            auth::start_recovery::tests::start_test_recovery,
            user::{
                get_secret::tests::get_test_secret,
                tests::{generate_random_bytes_len, get_test_uuid, hash_test_key, TestUser},
            },
        },
        tests::configure,
    };

    use super::{recovery, RecoverySchema};

    #[actix_web::test]
    async fn test_recover_reused_secret() {
        let secret = generate_random_bytes_len(crypto_box_SECRETKEYBYTES as usize);
        let (mut user, mail) = TestUser::random_with_secret(secret.clone()).await;
        user.login().await;
        let (cid, _) = user.add_random_charger().await;
        let recovery_id = start_test_recovery(&mail).await;

        let new_login_salt = generate_random_bytes_len(48);
        let new_password = generate_random_bytes_len(48);
        let new_login_key = hash_test_key(&new_password, &new_login_salt, None);
        let new_secret_salt = generate_random_bytes_len(48);
        let new_secret_nonce = generate_random_bytes_len(crypto_secretbox_NONCEBYTES as usize);
        let secret_key = hash_test_key(
            &new_password,
            &new_secret_salt,
            Some(crypto_secretbox_KEYBYTES as usize),
        );
        let mut new_encrypted_secret =
            vec![0u8; (crypto_secretbox_MACBYTES + crypto_box_SECRETKEYBYTES) as usize];
        unsafe {
            if crypto_secretbox_easy(
                new_encrypted_secret.as_mut_ptr(),
                secret.as_ptr(),
                secret.len() as u64,
                new_secret_nonce.as_ptr(),
                secret_key.as_ptr(),
            ) != 0
            {
                panic!("Encrypting secret failed.");
            }
        }

        let body = RecoverySchema {
            recovery_key: recovery_id.to_string(),
            new_login_key,
            new_login_salt,
            new_encrypted_secret,
            new_secret_nonce,
            new_secret_salt,
            reused_secret: true,
        };

        let app = App::new().configure(configure).service(recovery);
        let app = test::init_service(app).await;

        let req = TestRequest::post()
            .uri("/recovery")
            .set_json(body)
            .append_header(ContentType::json())
            .to_request();

        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 200);

        // "Logout user"
        user.access_token = None;
        user.password = new_password;
        let access_token = user.login().await.to_owned();

        let secret_resp = get_test_secret(&access_token).await;

        let secret_key = hash_test_key(
            &user.password,
            &secret_resp.secret_salt,
            Some(crypto_secretbox_KEYBYTES as usize),
        );
        let mut decrypted_secret = vec![0u8; crypto_box_SECRETKEYBYTES as usize];
        unsafe {
            if crypto_secretbox_open_easy(
                decrypted_secret.as_mut_ptr(),
                secret_resp.secret.as_ptr(),
                secret_resp.secret.len() as u64,
                secret_resp.secret_nonce.as_ptr(),
                secret_key.as_ptr(),
            ) != 0
            {
                panic!("Failed to decrypt secret")
            }
        }
        assert_eq!(decrypted_secret, secret);
        {
            use db_connector::schema::wg_keys::dsl::*;

            let pool = test_connection_pool();
            let mut conn = pool.get().unwrap();
            let res: Vec<WgKey> = wg_keys
                .filter(charger_id.eq(cid))
                .select(WgKey::as_select())
                .load(&mut conn)
                .unwrap();
            assert_eq!(res.len(), 5);
        }
    }

    #[actix_web::test]
    async fn test_recover_new_secret() {
        let (mut user, mail) = TestUser::random().await;
        user.login().await;
        let (cid, _) = user.add_random_charger().await;
        let recovery_id = start_test_recovery(&mail).await;

        let new_secret = generate_random_bytes_len(crypto_box_SECRETKEYBYTES as usize);
        let new_login_salt = generate_random_bytes_len(48);
        let new_password = generate_random_bytes_len(48);
        let new_login_key = hash_test_key(&new_password, &new_login_salt, None);
        let new_secret_salt = generate_random_bytes_len(48);
        let new_secret_nonce = generate_random_bytes_len(crypto_secretbox_NONCEBYTES as usize);
        let secret_key = hash_test_key(
            &new_password,
            &new_secret_salt,
            Some(crypto_secretbox_KEYBYTES as usize),
        );
        let mut new_encrypted_secret =
            vec![0u8; (crypto_secretbox_MACBYTES + crypto_box_SECRETKEYBYTES) as usize];
        unsafe {
            if crypto_secretbox_easy(
                new_encrypted_secret.as_mut_ptr(),
                new_secret.as_ptr(),
                new_secret.len() as u64,
                new_secret_nonce.as_ptr(),
                secret_key.as_ptr(),
            ) != 0
            {
                panic!("Encrypting secret failed.");
            }
        }

        let body = RecoverySchema {
            recovery_key: recovery_id.to_string(),
            new_login_key,
            new_login_salt,
            new_encrypted_secret,
            new_secret_nonce,
            new_secret_salt,
            reused_secret: false,
        };

        let app = App::new().configure(configure).service(recovery);
        let app = test::init_service(app).await;

        let req = TestRequest::post()
            .uri("/recovery")
            .set_json(body)
            .append_header(ContentType::json())
            .to_request();

        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 200);

        // "Logout user"
        user.access_token = None;
        user.password = new_password;
        let access_token = user.login().await.to_owned();

        let secret_resp = get_test_secret(&access_token).await;

        let secret_key = hash_test_key(
            &user.password,
            &secret_resp.secret_salt,
            Some(crypto_secretbox_KEYBYTES as usize),
        );
        let mut decrypted_secret = vec![0u8; crypto_box_SECRETKEYBYTES as usize];
        unsafe {
            if crypto_secretbox_open_easy(
                decrypted_secret.as_mut_ptr(),
                secret_resp.secret.as_ptr(),
                secret_resp.secret.len() as u64,
                secret_resp.secret_nonce.as_ptr(),
                secret_key.as_ptr(),
            ) != 0
            {
                panic!("Failed to decrypt secret")
            }
        }
        assert_eq!(decrypted_secret, new_secret);
        {
            use db_connector::schema::wg_keys::dsl::*;

            let pool = test_connection_pool();
            let mut conn = pool.get().unwrap();
            let res: Vec<WgKey> = wg_keys
                .filter(charger_id.eq(cid))
                .select(WgKey::as_select())
                .load(&mut conn)
                .unwrap();
            assert_eq!(res.len(), 0);
        }
        {
            use db_connector::schema::allowed_users::dsl::*;

            let pool = test_connection_pool();
            let mut conn = pool.get().unwrap();
            let res: Vec<AllowedUser> = allowed_users
                .filter(user_id.eq(get_test_uuid(&user.mail)))
                .filter(valid.eq(true))
                .select(AllowedUser::as_select())
                .load(&mut conn)
                .unwrap();
            assert_eq!(res.len(), 0);
        }
    }
}
