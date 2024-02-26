use actix_web::{post, web, HttpResponse, Responder};
use actix_web_validator::Json;
use argon2::{
    password_hash::{rand_core::OsRng, SaltString},
    Argon2, PasswordHasher,
};
use db_connector::models::{users::User, verification::Verification};
use diesel::{prelude::*, result::Error::NotFound};
use lettre::{message::header::ContentType, Message, SmtpTransport, Transport};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use validator::Validate;

use crate::{error::Error, utils::get_connection, AppState};

#[derive(Debug, Deserialize, Serialize, Validate, Clone, ToSchema)]
pub struct RegisterSchema {
    #[validate(length(min = 3))]
    pub name: String,
    #[validate(email)]
    pub email: String,
    #[validate(length(min = 12))]
    pub password: String,
}

pub fn hash_pass(password: &String) -> Result<String, String> {
    let salt = SaltString::generate(&mut OsRng);
    let hashed_password = match Argon2::default().hash_password(password.as_bytes(), &salt) {
        Ok(hash) => hash.to_string(),
        Err(err) => return Err(err.to_string()),
    };

    Ok(hashed_password)
}

// This is shown as unused in vscode since vscode assumes you have tests enabled.
#[allow(unused)]
fn send_verification_mail(
    id: Verification,
    email: String,
    mailer: SmtpTransport,
    frontend_url: String,
) -> Result<(), actix_web::Error> {
    let email = Message::builder()
        .from("Warp <warp@tinkerforge.com>".parse().unwrap())
        .to(email.parse().unwrap())
        .subject("Verify email")
        .header(ContentType::TEXT_PLAIN)
        .body(format!(
            "{}/api/auth/verify?id={}",
            frontend_url,
            id.id.to_string()
        ))
        .unwrap();

    match mailer.send(&email) {
        Ok(_) => println!("Email sent successfully!"),
        Err(e) => panic!("Could not send email: {e:?}"),
    }

    Ok(())
}

/// Register a new user
#[utoipa::path(
    context_path = "/auth",
    responses(
        (status = 201, description = "Registration was successful"),
        (status = 409, description = "A user with this email already exists")
    )
)]
#[post("/register")]
pub async fn register(
    state: web::Data<AppState>,
    data: Json<RegisterSchema>,
) -> Result<impl Responder, actix_web::Error> {
    use db_connector::schema::users::dsl::*;
    use db_connector::schema::verification::dsl::*;

    let mut conn = get_connection(&state)?;

    let user_mail = data.email.to_lowercase();
    let mail_cpy = user_mail.clone();

    let result = web::block(move || {
        users
            .filter(email.eq(mail_cpy))
            .select(User::as_select())
            .get_result(&mut conn)
    })
    .await
    .unwrap();

    match result {
        Err(NotFound) => (),
        Ok(_result) => return Err(Error::UserAlreadyExists.into()),
        Err(_err) => return Err(Error::InternalError.into()),
    };

    let password_hash = match hash_pass(&data.password) {
        Ok(hash) => hash,
        Err(_) => return Err(Error::InternalError.into()),
    };

    let user_insert = User {
        id: uuid::Uuid::new_v4(),
        name: data.name.clone(),
        password: password_hash,
        email: user_mail,
        email_verified: false,
    };

    let mut conn = get_connection(&state)?;

    let insert_result = match web::block(move || {
        let verify = Verification {
            id: uuid::Uuid::new_v4(),
            user: user_insert.id.clone(),
        };

        // same as above
        #[allow(unused)]
        let mail = user_insert.email.clone();

        let user_insert = diesel::insert_into(users)
            .values(&user_insert)
            .execute(&mut conn);
        match diesel::insert_into(verification)
            .values(&verify)
            .execute(&mut conn)
        {
            Ok(_) => (),
            Err(err) => return Err(err),
        }

        // maybe add mechanism to automatically retry?
        #[cfg(not(test))]
        std::thread::spawn(move || {
            send_verification_mail(
                verify,
                mail,
                state.mailer.clone(),
                state.frontend_url.clone(),
            )
            .ok();
        });

        user_insert
    })
    .await
    {
        Ok(result) => result,
        Err(_err) => return Err(Error::InternalError.into()),
    };

    match insert_result {
        Ok(_) => (),
        Err(_err) => return Err(Error::InternalError.into()),
    }

    Ok(HttpResponse::Created())
}

#[cfg(test)]
pub(crate) mod tests {
    use crate::{defer, tests::configure};
    use actix_web::{http::header::ContentType, test, App};

    use super::*;

    pub async fn create_user(mail: &str) {
        // test with valid syntax
        let app = App::new().configure(configure).service(register);
        let app = test::init_service(app).await;
        let user = RegisterSchema {
            name: "Test".to_string(),
            email: mail.to_string(),
            password: "TestTestTest".to_string(),
        };
        let req = test::TestRequest::post()
            .uri("/register")
            .insert_header(ContentType::json())
            .set_json(user)
            .to_request();
        let resp = test::call_service(&app, req).await;
        println!("{}", resp.status());
        assert!(resp.status().is_success());
        println!("Created user");
    }

    pub fn delete_user(mail: &str) {
        use db_connector::schema::users::dsl::*;
        use db_connector::schema::verification::dsl::*;
        use diesel::prelude::*;

        let pool = db_connector::test_connection_pool();
        let mut conn = pool.get().unwrap();
        let mail = mail.to_lowercase();
        let u: User = if let Ok(u) = users
            .filter(email.eq(mail.clone()))
            .select(User::as_select())
            .get_result(&mut conn)
        {
            u
        } else {
            return;
        };

        diesel::delete(verification.filter(user.eq(u.id)))
            .execute(&mut conn)
            .expect("Error deleting verification");
        diesel::delete(users.filter(email.eq(mail.to_lowercase())))
            .execute(&mut conn)
            .expect("Error deleting test tuser");
    }

    fn user_exists(mail: &str) -> bool {
        use db_connector::schema::users::dsl::*;
        use diesel::prelude::*;

        let pool = db_connector::test_connection_pool();
        match users
            .filter(email.eq(mail))
            .select(User::as_select())
            .get_result(&mut pool.get().unwrap())
        {
            Ok(_) => true,
            Err(NotFound) => false,
            Err(err) => panic!("Something went wrong: {}", err),
        }
    }

    #[actix_web::test]
    async fn test_no_data() {
        // Test without data
        let app = App::new().configure(configure).service(register);
        let app = test::init_service(app).await;
        let req = test::TestRequest::post()
            .uri("/register")
            .insert_header(ContentType::json())
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_client_error());
    }

    #[actix_web::test]
    async fn test_short_password() {
        // test with to short password
        let app = App::new().configure(configure).service(register);
        let app = test::init_service(app).await;
        let mail = "Test@test.invalid";
        let user = RegisterSchema {
            name: "Test".to_string(),
            email: mail.to_string(),
            password: "Test".to_string(),
        };
        let req = test::TestRequest::post()
            .uri("/register")
            .insert_header(ContentType::json())
            .set_json(user)
            .to_request();
        let resp = test::call_service(&app, req).await;

        assert!(resp.status().is_client_error());
        assert_eq!(false, user_exists(mail));
    }

    #[actix_web::test]
    async fn test_invalid_email() {
        // test with invalid email
        let app = App::new().configure(configure).service(register);
        let app = test::init_service(app).await;

        let mail = "Testtest.de";
        let user = RegisterSchema {
            name: "Test".to_string(),
            email: mail.to_string(),
            password: "TestTestTest".to_string(),
        };
        let req = test::TestRequest::post()
            .uri("/register")
            .insert_header(ContentType::json())
            .set_json(user)
            .to_request();
        let resp = test::call_service(&app, req).await;

        assert!(resp.status().is_client_error());
        assert_eq!(false, user_exists(mail));
    }

    #[actix_web::test]
    async fn test_short_username() {
        // test with too short username
        let app = App::new().configure(configure).service(register);
        let app = test::init_service(app).await;

        let mail = "Test@test.invalid";
        let user = RegisterSchema {
            name: "Te".to_string(),
            email: mail.to_string(),
            password: "TestTestTest".to_string(),
        };
        let req = test::TestRequest::post()
            .uri("/register")
            .insert_header(ContentType::json())
            .set_json(user)
            .to_request();
        let resp = test::call_service(&app, req).await;

        assert!(resp.status().is_client_error());
        assert_eq!(false, user_exists(mail));
    }

    #[actix_web::test]
    async fn test_valid_request() {
        // test with valid syntax
        let app = App::new().configure(configure).service(register);
        let app = test::init_service(app).await;
        let mail = "valid_request@test.invalid";
        let user = RegisterSchema {
            name: "Test".to_string(),
            email: mail.to_string(),
            password: "TestTestTest".to_string(),
        };
        let req = test::TestRequest::post()
            .uri("/register")
            .insert_header(ContentType::json())
            .set_json(user)
            .to_request();
        let resp = test::call_service(&app, req).await;
        println!("{}", resp.status());

        assert!(resp.status().is_success());
        assert_eq!(true, user_exists(mail));
        delete_user("valid_request@test.invalid");
    }

    #[actix_web::test]
    async fn test_existing_user() {
        let app = App::new().configure(configure).service(register);
        let app = test::init_service(app).await;
        let mail = "existing_user@test.invalid".to_string();
        let user = RegisterSchema {
            name: "Test".to_string(),
            email: mail.clone(),
            password: "TestTestTest".to_string(),
        };
        let req = test::TestRequest::post()
            .uri("/register")
            .insert_header(ContentType::json())
            .set_json(user.clone())
            .to_request();
        let resp = test::call_service(&app, req).await;
        println!("{}", resp.status());
        assert!(resp.status().is_success());
        defer!(delete_user(mail.as_str()));

        let req = test::TestRequest::post()
            .uri("/register")
            .insert_header(ContentType::json())
            .set_json(user)
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_client_error());
    }
}
