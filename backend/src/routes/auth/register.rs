use actix_web::{post, web, HttpResponse, Responder};
use argon2::{password_hash::{rand_core::OsRng, SaltString}, Argon2, PasswordHasher};
use db_connector::model::users::User;
use diesel::prelude::*;
use actix_web_validator::Json;

use crate::{model::register::RegisterSchema, AppState};

fn hash_pass(password: &String) -> Result<String, String> {
    let salt = SaltString::generate(&mut OsRng);
    let hashed_password = match Argon2::default()
        .hash_password(password.as_bytes(), &salt) {
            Ok(hash) => hash.to_string(),
            Err(err) => {
                return Err(err.to_string())
            }
        };

    Ok(hashed_password)
}

#[post("/register")]
pub async fn register(state: web::Data<AppState>, data: Json<RegisterSchema>) -> impl Responder {
    use db_connector::schema::users::dsl::*;

    let mut conn = match state.pool.get() {
        Ok(conn) => conn,
        Err(_err) => {
            return HttpResponse::InternalServerError()
        }
    };

    let result = match users.filter(email.eq(&data.email))
        .select(User::as_select())
        .load(&mut conn) {
            Ok(result) => result,
            Err(_err) => {
                return HttpResponse::InternalServerError()
            }
        };

    if result.len() != 0 {
        return HttpResponse::Conflict()
    }

    let password_hash = match hash_pass(&data.password) {
        Ok(hash) => hash,
        Err(_) => return HttpResponse::InternalServerError()
    };

    let user = User {
        id: uuid::Uuid::new_v4(),
        name: data.name.clone(),
        password: password_hash,
        email: data.email.clone(),
        email_verified: false
    };

    let insert_result = diesel::insert_into(users)
        .values(&user)
        .execute(&mut conn);

    match insert_result {
        Ok(_) => (),
        Err(_err) => {
            return HttpResponse::InternalServerError()
        }
    }

    HttpResponse::Created()
}


#[cfg(test)]
mod tests {
    use actix_web::{http::header::ContentType, test, web::ServiceConfig, App};
    use super::*;

    fn configure(cfg: &mut ServiceConfig) {
        let pool = db_connector::get_connection_pool();
        let state = AppState {
            pool
        };
        let state = web::Data::new(state);
        cfg.app_data(state.clone());
    }

    #[actix_web::test]
    async fn test_no_data() {
        // Test without data
        let app = App::new().configure(configure ).service(register);
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
        let app = App::new().configure(configure ).service(register);
        let app = test::init_service(app).await;
        let user = RegisterSchema {
            name: "Test".to_string(),
            email: "Test@test.de".to_string(),
            password: "Test".to_string()
        };
        let req = test::TestRequest::post()
            .uri("/register")
            .insert_header(ContentType::json())
            .set_json(user)
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_client_error());
    }

    #[actix_web::test]
    async fn test_invalid_email() {
        // test with invalid email
        let app = App::new().configure(configure ).service(register);
        let app = test::init_service(app).await;
        let user = RegisterSchema {
            name: "Test".to_string(),
            email: "Testtest.de".to_string(),
            password: "TestTestTest".to_string()
        };
        let req = test::TestRequest::post()
            .uri("/register")
            .insert_header(ContentType::json())
            .set_json(user)
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_client_error());

    }

    #[actix_web::test]
    async fn test_short_username() {
        // test with too short username
        let app = App::new().configure(configure ).service(register);
        let app = test::init_service(app).await;
        let user = RegisterSchema {
            name: "Te".to_string(),
            email: "Test@test.de".to_string(),
            password: "TestTestTest".to_string()
        };
        let req = test::TestRequest::post()
            .uri("/register")
            .insert_header(ContentType::json())
            .set_json(user)
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_client_error());
    }

    fn delete_test_user() {
        use crate::schema::users::dsl::*;
        let pool = db_connector::get_connection_pool();
        let mut conn = pool.get().unwrap();
        diesel::delete(users.filter(name.eq("Test"))).execute(&mut conn).expect("Error deleting test tuser");
    }

    #[actix_web::test]
    async fn test_valid_request() {
        // test with valid syntax
        let app = App::new().configure(configure ).service(register);
        let app = test::init_service(app).await;
        let user = RegisterSchema {
            name: "Test".to_string(),
            email: "Test@test.de".to_string(),
            password: "TestTestTest".to_string()
        };
        let req = test::TestRequest::post()
            .uri("/register")
            .insert_header(ContentType::json())
            .set_json(user)
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_success());
        delete_test_user();
    }

    #[actix_web::test]
    async fn test_existing_user() {
        let app = App::new().configure(configure ).service(register);
        let app = test::init_service(app).await;
        let user = RegisterSchema {
            name: "Test".to_string(),
            email: "Test@test.de".to_string(),
            password: "TestTestTest".to_string()
        };
        let req = test::TestRequest::post()
            .uri("/register")
            .insert_header(ContentType::json())
            .set_json(user.clone())
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_success());

        let req = test::TestRequest::post()
            .uri("/register")
            .insert_header(ContentType::json())
            .set_json(user)
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_client_error());
        delete_test_user();
    }
}
