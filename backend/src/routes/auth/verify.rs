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

use std::str::FromStr;

use actix_web::{
    error::ErrorBadRequest,
    get,
    web::{self, Redirect},
    Responder,
};
use db_connector::models::verification::Verification;
use diesel::{ExpressionMethods, QueryDsl, SelectableHelper};
use diesel_async::RunQueryDsl as _;
use serde::Deserialize;
use utoipa::IntoParams;

use crate::{error::Error, utils::get_connection, AppState};

#[derive(Deserialize, IntoParams)]
struct Query {
    /// Verification id that was sent to the user via email.
    pub id: String,
}

/// Verify a registered user.
#[utoipa::path(
    context_path = "/auth",
    params(
        Query
    ),
    responses(
        (status = 307, description = "Verification was successful and a redirect to the login is sent."),
        (status = 400, description = "There is no verification request or the account was already verified.")
    )
)]
#[get("/verify")]
pub async fn verify(state: web::Data<AppState>, ver: web::Query<Query>) -> impl Responder {
    use db_connector::schema::users::dsl::*;
    use db_connector::schema::verification::dsl::*;

    let mut conn = get_connection(&state).await?;

    let verify_id = match uuid::Uuid::from_str(&ver.id) {
        Ok(verify_id) => verify_id,
        Err(err) => return Err(ErrorBadRequest(err)),
    };

    let verify: Verification = verification
        .filter(db_connector::schema::verification::id.eq(verify_id))
        .select(Verification::as_select())
        .get_result::<Verification>(&mut conn)
        .await
        .map_err(|err| match err {
            diesel::result::Error::NotFound => Error::InternalError,
            _ => Error::InternalError,
        })
        .map_err(|_| ErrorBadRequest("Account was already verified or does not exist"))?;

    let mut conn = get_connection(&state).await?;

    if let Err(_err) = diesel::update(users.find(verify.user))
        .set((
            email_verified.eq(true),
            old_email.eq::<Option<String>>(None),
            old_delivery_email.eq::<Option<String>>(None),
        ))
        .execute(&mut conn)
        .await
    {
        return Err(Error::InternalError.into());
    }

    if let Err(_err) = diesel::delete(verification.find(verify.id))
        .execute(&mut conn)
        .await
    {
        return Err(Error::InternalError.into());
    }

    Ok(Redirect::to(format!(
        "{}?verified=true",
        state.frontend_url
    )))
}

#[cfg(test)]
pub(crate) mod tests {
    use actix_web::{test, App};
    use db_connector::models::{users::User, verification::Verification};
    use diesel::{result::Error::NotFound, ExpressionMethods, QueryDsl, SelectableHelper};
    use diesel_async::{AsyncPgConnection, RunQueryDsl as _AsyncRunQueryDsl};

    use crate::{
        defer_async,
        routes::{
            auth::register::tests::{create_user, delete_user},
            user::{me::tests::get_test_user, tests::TestUser},
        },
        tests::configure,
    };

    pub async fn fast_verify(mail: &str) {
        let pool = db_connector::test_connection_pool();
        let mut conn = pool.get().await.unwrap();
        let mail_owned = mail.to_string();
        {
            use db_connector::schema::users::dsl::*;
            use db_connector::schema::verification::dsl::verification;

            // Keep the lookup and updates on the same connection so this
            // helper remains atomic and does not depend on pool capacity.
            let verify_id = get_verify_id(&mut conn, &mail_owned).await;
            diesel::delete(verification.find(verify_id))
                .execute(&mut conn)
                .await
                .unwrap();
            diesel::update(users.filter(email.eq(&mail_owned)))
                .set(email_verified.eq(true))
                .execute(&mut conn)
                .await
                .unwrap();
        }
    }

    async fn get_verify_id(conn: &mut AsyncPgConnection, mail: &str) -> uuid::Uuid {
        use db_connector::schema::users::dsl::{email, users};
        use db_connector::schema::verification::dsl::*;

        println!("mail: {mail}");

        let u: User = users
            .filter(email.eq(mail))
            .select(User::as_select())
            .get_result(conn)
            .await
            .unwrap();
        let verify: Verification = verification
            .filter(user.eq(u.id))
            .select(Verification::as_select())
            .get_result(conn)
            .await
            .unwrap();

        verify.id
    }

    async fn check_for_verify(conn: &mut AsyncPgConnection, verify: &uuid::Uuid) -> bool {
        use db_connector::schema::verification::dsl::*;

        match verification
            .find(verify)
            .select(Verification::as_select())
            .get_result::<Verification>(conn)
            .await
        {
            Ok(_) => true,
            Err(NotFound) => false,
            Err(err) => panic!("Something went wrong: {err}"),
        }
    }

    #[actix_web::test]
    async fn test_valid_verify() {
        let mail = "valid_verify@test.invalid";
        create_user(mail).await;
        defer_async!(delete_user(mail));

        let pool = db_connector::test_connection_pool();
        let verify_id = {
            let mut conn = pool.get().await.unwrap();
            let mail_owned = mail.to_string();
            get_verify_id(&mut conn, &mail_owned).await
        };

        let app = App::new().configure(configure).service(super::verify);
        let app = test::init_service(app).await;

        let req = test::TestRequest::get()
            .uri(&format!("/verify?id={verify_id}"))
            .to_request();

        let resp = test::call_service(&app, req).await;

        println!("{}", resp.status());
        assert!(resp.status().is_redirection());
        let mut conn = pool.get().await.unwrap();
        assert!(!check_for_verify(&mut conn, &verify_id).await);
    }

    #[actix_web::test]
    async fn test_invalid_verify() {
        let mail = "invalid_verify@test.invalid";
        create_user(mail).await;
        defer_async!(delete_user(mail));

        let pool = db_connector::test_connection_pool();
        let verify_id = {
            let mut conn = pool.get().await.unwrap();
            let mail_owned = mail.to_string();
            get_verify_id(&mut conn, &mail_owned).await
        };

        let app = App::new().configure(configure).service(super::verify);
        let app = test::init_service(app).await;

        let req = test::TestRequest::get()
            .uri(&format!("/verify?id={}", uuid::Uuid::new_v4()))
            .to_request();

        let resp = test::call_service(&app, req).await;

        assert!(resp.status().is_client_error());
        let mut conn = pool.get().await.unwrap();
        assert!(check_for_verify(&mut conn, &verify_id).await);
    }

    #[actix_web::test]
    async fn test_no_verify() {
        let mail = "no_verify@test.invalid";
        create_user(mail).await;
        defer_async!(delete_user(mail));

        let pool = db_connector::test_connection_pool();
        let verify_id = {
            let mut conn = pool.get().await.unwrap();
            let mail_owned = mail.to_string();
            get_verify_id(&mut conn, &mail_owned).await
        };

        let app = App::new().configure(configure).service(super::verify);
        let app = test::init_service(app).await;

        let req = test::TestRequest::get()
            .uri(&format!("/verify?i={verify_id}"))
            .to_request();

        let resp = test::call_service(&app, req).await;

        assert!(resp.status().is_client_error());
        let mut conn = pool.get().await.unwrap();
        assert!(check_for_verify(&mut conn, &verify_id).await);

        let req = test::TestRequest::get().uri("/verify?").to_request();

        let resp = test::call_service(&app, req).await;

        assert!(resp.status().is_client_error());
        assert!(check_for_verify(&mut conn, &verify_id).await);
    }

    #[actix_web::test]
    async fn test_verify_changed_email() {
        let (mut user, mail) = TestUser::random().await;
        user.login().await.to_string();

        let changed_mail = format!("changed_{mail}");
        let changed_mail_cpy = changed_mail.clone();
        defer_async!({
            let inner_mail = changed_mail_cpy;
            async move { delete_user(&inner_mail).await }
        });
        let pool = db_connector::test_connection_pool();
        {
            let mut conn = pool.get().await.unwrap();
            let mail_owned = mail.clone();
            let changed_owned = changed_mail.clone();
            use db_connector::schema::users::dsl::*;

            diesel::update(users.filter(email.eq(&mail_owned)))
                .set((
                    old_email.eq(Some(&mail_owned)),
                    old_delivery_email.eq(Some(&mail_owned)),
                    email_verified.eq(false),
                    email.eq(&changed_owned),
                    delivery_email.eq(&changed_owned),
                ))
                .execute(&mut conn)
                .await
                .unwrap();
        }

        let verify = {
            let user_uuid = get_test_user(&changed_mail).await.id;
            let mut conn = pool.get().await.unwrap();
            use db_connector::schema::verification::dsl::*;

            let verify = Verification::new(user_uuid);
            diesel::insert_into(verification)
                .values(&verify)
                .execute(&mut conn)
                .await
                .unwrap();
            verify
        };

        let app = App::new().configure(configure).service(super::verify);
        let app = test::init_service(app).await;

        let req = test::TestRequest::get()
            .uri(&format!("/verify?id={}", verify.id))
            .to_request();

        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_redirection());

        {
            let mut conn = pool.get().await.unwrap();
            let changed_owned = changed_mail.clone();
            use db_connector::schema::users::dsl::*;

            let u: User = users
                .filter(email.eq(changed_owned))
                .select(User::as_select())
                .get_result(&mut conn)
                .await
                .unwrap();
            assert!(u.email_verified);
            assert_eq!(None, u.old_email);
            assert_eq!(None, u.old_delivery_email);
        }

        let mut conn = pool.get().await.unwrap();
        assert!(!check_for_verify(&mut conn, &verify.id).await);
    }
}
