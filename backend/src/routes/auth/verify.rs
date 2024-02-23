use std::str::FromStr;

use actix_web::{
    error::ErrorBadRequest,
    get,
    web::{self, Redirect},
    Responder,
};
use db_connector::models::verification::Verification;
use diesel::prelude::*;
use serde::Deserialize;

use crate::{error::Error, utils::get_connection, AppState};

#[derive(Deserialize)]
struct Query {
    pub id: String,
}

#[get("/verify")]
pub async fn verify(state: web::Data<AppState>, ver: web::Query<Query>) -> impl Responder {
    use db_connector::schema::users::dsl::*;
    use db_connector::schema::verification::dsl::*;

    let mut conn = get_connection(&state)?;

    let verify_id = match uuid::Uuid::from_str(&ver.id) {
        Ok(verify_id) => verify_id,
        Err(err) => return Err(ErrorBadRequest(err)),
    };

    let result = match web::block(move || {
        verification
            .filter(db_connector::schema::verification::id.eq(verify_id))
            .select(Verification::as_select())
            .get_result(&mut conn)
    })
    .await
    {
        Ok(result) => result,
        Err(_err) => return Err(Error::InternalError.into()),
    };

    let verify: Verification = match result {
        Ok(verify) => verify,
        Err(_err) => {
            return Err(ErrorBadRequest(
                "Account was already veryfied or does not exist",
            ))
        }
    };

    let mut conn = get_connection(&state)?;

    match web::block(move || {
        if let Err(_err) = diesel::update(users.find(verify.user))
            .set(email_verified.eq(true))
            .execute(&mut conn)
        {
            return Err::<(), Error>(Error::InternalError.into());
        }

        if let Err(_err) = diesel::delete(verification.find(verify.id)).execute(&mut conn) {
            return Err::<(), Error>(Error::InternalError.into());
        }

        Ok(())
    })
    .await
    {
        Ok(res) => match res {
            Ok(()) => (),
            Err(err) => return Err(err.into()),
        },
        Err(_) => return Err(Error::InternalError.into()),
    }

    Ok(Redirect::to(state.frontend_url.clone()))
}

#[cfg(test)]
pub(crate) mod tests {
    use actix_web::{test, App};
    use db_connector::models::{users::User, verification::Verification};
    use diesel::{
        prelude::*,
        r2d2::{ConnectionManager, PooledConnection},
        result::Error::NotFound,
        PgConnection, SelectableHelper,
    };

    use crate::{
        defer,
        routes::auth::register::tests::{create_user, delete_user},
        tests::configure,
    };

    pub fn fast_verify(mail: &str) {
        use db_connector::schema::users::dsl::*;
        use db_connector::schema::verification::dsl::verification;

        println!("Fast verify for {}", mail);
        let pool = db_connector::test_connection_pool();
        let mut conn = pool.get().unwrap();
        let verify = get_verify_id(&mut conn, mail);
        diesel::delete(verification.find(verify))
            .execute(&mut conn)
            .unwrap();
        diesel::update(users.filter(email.eq(mail)))
            .set(email_verified.eq(true))
            .execute(&mut conn)
            .unwrap();
    }

    fn get_verify_id(
        conn: &mut PooledConnection<ConnectionManager<PgConnection>>,
        mail: &str,
    ) -> uuid::Uuid {
        use db_connector::schema::users::dsl::{email, users};
        use db_connector::schema::verification::dsl::*;

        let u: User = users
            .filter(email.eq(mail))
            .select(User::as_select())
            .get_result(conn)
            .unwrap();
        let verify: Verification = verification
            .filter(user.eq(u.id))
            .select(Verification::as_select())
            .get_result(conn)
            .unwrap();

        verify.id
    }

    fn check_for_verify(
        conn: &mut PooledConnection<ConnectionManager<PgConnection>>,
        verify: &uuid::Uuid,
    ) -> bool {
        use db_connector::schema::verification::dsl::*;

        match verification
            .find(verify)
            .select(Verification::as_select())
            .get_result(conn)
        {
            Ok(_) => true,
            Err(NotFound) => false,
            Err(err) => panic!("Something went wrong: {}", err),
        }
    }

    #[actix_web::test]
    async fn test_valid_verify() {
        let mail = "valid_verify@test.invalid";
        create_user(mail).await;
        defer!(delete_user(mail));

        let pool = db_connector::test_connection_pool();
        let mut conn = pool.get().unwrap();
        let verify_id = get_verify_id(&mut conn, mail);

        let app = App::new().configure(configure).service(super::verify);
        let app = test::init_service(app).await;

        let req = test::TestRequest::get()
            .uri(&format!("/verify?id={}", verify_id.to_string()))
            .to_request();

        let resp = test::call_service(&app, req).await;

        println!("{}", resp.status());
        assert!(resp.status().is_redirection());
        assert_eq!(false, check_for_verify(&mut conn, &verify_id));
    }

    #[actix_web::test]
    async fn test_invalid_verify() {
        let mail = "invalid_verify@test.invalid";
        create_user(mail).await;
        defer!(delete_user(mail));

        let pool = db_connector::test_connection_pool();
        let mut conn = pool.get().unwrap();
        let verify_id = get_verify_id(&mut conn, mail);

        let app = App::new().configure(configure).service(super::verify);
        let app = test::init_service(app).await;

        let req = test::TestRequest::get()
            .uri(&format!("/verify?id={}", uuid::Uuid::new_v4().to_string()))
            .to_request();

        let resp = test::call_service(&app, req).await;

        assert!(resp.status().is_client_error());
        assert_eq!(true, check_for_verify(&mut conn, &verify_id));
    }

    #[actix_web::test]
    async fn test_no_verify() {
        let mail = "no_verify@test.invalid";
        create_user(mail).await;
        defer!(delete_user(mail));

        let pool = db_connector::test_connection_pool();
        let mut conn = pool.get().unwrap();
        let verify_id = get_verify_id(&mut conn, mail);

        let app = App::new().configure(configure).service(super::verify);
        let app = test::init_service(app).await;

        let req = test::TestRequest::get()
            .uri(&format!("/verify?i={}", verify_id.to_string()))
            .to_request();

        let resp = test::call_service(&app, req).await;

        assert!(resp.status().is_client_error());
        assert_eq!(true, check_for_verify(&mut conn, &verify_id));

        let req = test::TestRequest::get()
            .uri(&format!("/verify?"))
            .to_request();

        let resp = test::call_service(&app, req).await;

        assert!(resp.status().is_client_error());
        assert_eq!(true, check_for_verify(&mut conn, &verify_id));
    }
}
