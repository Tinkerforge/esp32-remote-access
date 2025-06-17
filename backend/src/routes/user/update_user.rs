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

use crate::{
    error::Error,
    routes::auth::VERIFICATION_EXPIRATION_DAYS,
    utils::{get_connection, send_email, web_block_unpacked},
    AppState,
};
use actix_web::{error::ErrorConflict, put, web, HttpResponse, Responder};
use askama::Template;
use db_connector::models::users::User;
use diesel::{prelude::*, result::Error::NotFound};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use validator::Validate;

#[allow(unused)]
#[derive(Template)]
#[template(path = "email_change_notification_en.html")]
struct EmailChangeNotificationEn {
    name: String,
    sender_email: String,
}

#[allow(unused)]
#[derive(Template)]
#[template(path = "email_change_notification_de.html")]
struct EmailChangeNotificationDe {
    name: String,
    sender_email: String,
}

#[allow(unused)]
fn send_email_change_notification(
    name: String,
    old_email: String,
    lang: String,
    state: web::Data<AppState>,
) {
    std::thread::spawn(move || {
        let (body, subject) = match lang.as_str() {
            "de" => {
                let template = EmailChangeNotificationDe {
                    name: name.to_string(),
                    sender_email: state.sender_email.clone(),
                };
                (template.render().unwrap(), "E-Mail-Adresse geändert")
            }
            _ => {
                let template = EmailChangeNotificationEn {
                    name: name.to_string(),
                    sender_email: state.sender_email.clone(),
                };
                (template.render().unwrap(), "Email address changed")
            }
        };
        send_email(&old_email, subject, body, &state);
    });
}

#[allow(unused)]
fn send_verification_mail(
    name: String,
    email: String,
    lang: String,
    state: web::Data<AppState>,
    verification_id: uuid::Uuid,
) {
    std::thread::spawn(move || {
        let (body, subject) = match lang.as_str() {
            "de" => {
                let template = crate::routes::auth::register::VerifyEmailDETemplate {
                    name: &name,
                    link: &format!(
                        "{}/api/auth/verify?id={}",
                        state.frontend_url, verification_id
                    ),
                };
                (template.render().unwrap(), "E-Mail-Adresse bestätigen")
            }
            _ => {
                let template = crate::routes::auth::register::VerifyEmailENTemplate {
                    name: &name,
                    link: &format!(
                        "{}/api/auth/verify?id={}",
                        state.frontend_url, verification_id
                    ),
                };
                (template.render().unwrap(), "Verify email address")
            }
        };

        send_email(&email, subject, body, &state);
    });
}

#[derive(Serialize, Deserialize, ToSchema, Validate, Clone)]
pub struct UpdateUserSchema {
    #[validate(length(min = 3))]
    pub name: String,
    #[validate(email)]
    pub email: String,
}

/// Update basic user information.
#[utoipa::path(
    context_path = "/user",
    request_body = UpdateUserSchema,
    responses(
        (status = 200, description = "Update was successful.")
    ),
    security(
        ("jwt" = [])
    )
)]
#[put("/update_user")]
pub async fn update_user(
    state: web::Data<AppState>,
    new_user: actix_web_validator::Json<UpdateUserSchema>,
    uid: crate::models::uuid::Uuid,
    #[cfg(not(test))] lang: crate::models::lang::Lang,
) -> Result<impl Responder, actix_web::Error> {
    use db_connector::schema::users::dsl::*;

    let uid: uuid::Uuid = uid.into();
    let user_cpy = new_user.clone();
    let mut conn = get_connection(&state)?;
    web_block_unpacked(move || {
        match users
            .filter(email.eq(&user_cpy.email.to_lowercase()))
            .select(User::as_select())
            .get_result(&mut conn) as Result<User, diesel::result::Error>
        {
            Err(NotFound) => (),
            Ok(u) => {
                if u.id != uid {
                    return Err(Error::UserAlreadyExists);
                }
            }
            Err(_err) => return Err(Error::InternalError),
        }

        Ok(())
    })
    .await?;

    let mut conn = get_connection(&state)?;
    let old_user: User = web_block_unpacked(move || {
        match users
            .find::<uuid::Uuid>(uid)
            .select(User::as_select())
            .get_result(&mut conn)
        {
            Ok(u) => Ok(u),
            Err(NotFound) => Err(Error::Unauthorized),
            Err(_err) => Err(Error::InternalError),
        }
    })
    .await?;

    let mut conn = get_connection(&state)?;
    // Only set up verification if email changed
    let exp = if new_user.email != old_user.email {
        if old_user.old_email.is_some() {
            return Err(ErrorConflict("Another email change is already pending."));
        }

        if let Some(expiration) =
            chrono::Utc::now().checked_add_days(chrono::Days::new(VERIFICATION_EXPIRATION_DAYS))
        {
            Some(expiration.naive_utc())
        } else {
            return Err(Error::InternalError.into());
        }
    } else {
        None
    };

    web_block_unpacked(move || {
        // Update user fields
        match diesel::update(users.find::<uuid::Uuid>(uid))
            .set((
                name.eq(&new_user.name),
                email.eq(&new_user.email.to_lowercase()),
                delivery_email.eq(&new_user.email),
                email_verified.eq(new_user.email == old_user.email),
                old_email.eq(&old_user.email),
                old_delivery_email.eq(&old_user.delivery_email),
            ))
            .execute(&mut conn)
        {
            Ok(_) => (),
            Err(NotFound) => return Err(Error::Unauthorized),
            Err(_err) => return Err(Error::InternalError),
        }

        if let Some(exp) = exp {
            use db_connector::schema::verification::dsl::*;

            let verify = db_connector::models::verification::Verification {
                id: uuid::Uuid::new_v4(),
                user: uid,
                expiration: exp,
            };

            // Insert verification record
            match diesel::insert_into(verification)
                .values(&verify)
                .execute(&mut conn)
            {
                Ok(_) => (),
                Err(_err) => return Err(Error::InternalError),
            }

            #[cfg(not(test))]
            {
                let lang: String = lang.into();
                send_verification_mail(
                    new_user.name.clone(),
                    new_user.email.clone(),
                    lang.clone(),
                    state.clone(),
                    verify.id,
                );
                let old_user_email = old_user.delivery_email.unwrap_or(old_user.email);
                send_email_change_notification(old_user.name, old_user_email, lang, state);
            }
        }

        Ok(())
    })
    .await?;

    Ok(HttpResponse::Ok())
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use crate::{
        defer,
        routes::{
            auth::{
                login::tests::verify_and_login_user,
                register::tests::{create_user, delete_user},
            },
            user::{me::tests::get_test_user, tests::TestUser},
        },
        tests::configure,
    };
    use actix_web::{cookie::Cookie, test, App};
    use db_connector::test_connection_pool;

    pub async fn update_test_user(token: String, update: UpdateUserSchema) {
        let app = App::new()
            .configure(configure)
            .service(update_user)
            .wrap(crate::middleware::jwt::JwtMiddleware);
        let app = test::init_service(app).await;

        let req = test::TestRequest::put()
            .uri("/update_user")
            .set_json(update)
            .cookie(Cookie::new("access_token", token))
            .to_request();

        test::call_service(&app, req).await;
    }

    #[actix_web::test]
    async fn test_update_email() {
        let mail = "update_mail@test.invalid";
        let key = create_user(mail).await;
        defer!(delete_user(mail));
        let update_mail = format!("t{}", mail);
        defer!(delete_user(&update_mail));

        let app = App::new()
            .configure(configure)
            .service(update_user)
            .wrap(crate::middleware::jwt::JwtMiddleware);
        let app = test::init_service(app).await;

        let user = get_test_user(mail);
        let mut user = user;
        let old_user = user.clone();
        user.email = update_mail.clone();
        let user_schema = UpdateUserSchema {
            name: user.name,
            email: user.email,
        };

        let (token, _) = verify_and_login_user(mail, key).await;
        let req = test::TestRequest::put()
            .uri("/update_user")
            .set_json(user_schema)
            .cookie(Cookie::new("access_token", token))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_success());

        // Check that email_verified is false after email change
        let updated_user = get_test_user(&update_mail);
        assert!(!updated_user.email_verified);
        assert_eq!(old_user.email, updated_user.old_email.unwrap());
        assert_eq!(old_user.delivery_email, updated_user.old_delivery_email);

        let pool = test_connection_pool();
        let mut conn = pool.get().unwrap();
        {
            // Check that verification record was created
            use db_connector::schema::verification::dsl::*;
            let verify_record = verification
                .filter(user.eq(updated_user.id))
                .select(db_connector::models::verification::Verification::as_select())
                .get_result(&mut conn)
                .unwrap();
            assert!(verify_record.expiration > chrono::Utc::now().naive_utc());
        }
    }

    #[actix_web::test]
    async fn test_existing_email() {
        let (mut user, mail) = TestUser::random().await;
        let (_user2, mail2) = TestUser::random().await;
        let token = user.login().await;
        let app = App::new()
            .configure(configure)
            .service(update_user)
            .wrap(crate::middleware::jwt::JwtMiddleware);
        let app = test::init_service(app).await;

        let user = get_test_user(&mail);
        let mut user = user;
        user.email = mail2;
        let user = UpdateUserSchema {
            name: user.name,
            email: user.email,
        };

        let req = test::TestRequest::put()
            .uri("/update_user")
            .set_json(user)
            .cookie(Cookie::new("access_token", token))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_client_error());
    }

    #[actix_web::test]
    async fn test_update_name_keeps_verification() {
        let (mut user, mail) = TestUser::random().await;
        let token = user.login().await;

        let app = App::new()
            .configure(configure)
            .service(update_user)
            .wrap(crate::middleware::jwt::JwtMiddleware);
        let app = test::init_service(app).await;

        // Get current user and change only name
        let db_user = get_test_user(&mail);
        let update = UpdateUserSchema {
            name: "New Name".to_string(),
            email: db_user.email.clone(),
        };

        let req = test::TestRequest::put()
            .uri("/update_user")
            .set_json(update)
            .cookie(Cookie::new("access_token", token))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_success());

        // Check that email_verified is still true
        let updated_user = get_test_user(&mail);
        assert!(updated_user.email_verified);

        let pool = test_connection_pool();
        let mut conn = pool.get().unwrap();
        {
            // Verify no verification record was created
            use db_connector::schema::verification::dsl::*;
            let verify_records = verification
                .filter(user.eq(updated_user.id))
                .select(db_connector::models::verification::Verification::as_select())
                .load::<db_connector::models::verification::Verification>(&mut conn)
                .unwrap();
            assert!(verify_records.is_empty());
        }
    }

    #[actix_web::test]
    async fn test_pending_email_change() {
        let (mut user, mail) = TestUser::random().await;
        let token = user.login().await;

        let app = App::new()
            .configure(configure)
            .service(update_user)
            .wrap(crate::middleware::jwt::JwtMiddleware);
        let app = test::init_service(app).await;

        // Get current user and change email first time
        let db_user = get_test_user(&mail);
        let new_email = format!("changed_{}", mail);
        let update = UpdateUserSchema {
            name: db_user.name.clone(),
            email: new_email.clone(),
        };

        defer!(delete_user(&new_email));

        let req = test::TestRequest::put()
            .uri("/update_user")
            .set_json(update)
            .cookie(Cookie::new("access_token", token))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_success());

        // Attempt second email change while first is pending
        let another_email = format!("another_{}", mail);
        let update = UpdateUserSchema {
            name: db_user.name,
            email: another_email,
        };

        let req = test::TestRequest::put()
            .uri("/update_user")
            .set_json(update)
            .cookie(Cookie::new("access_token", token))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status().as_u16(), 409); // Conflict status code
    }
}
