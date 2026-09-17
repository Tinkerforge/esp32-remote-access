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
    branding,
    error::Error,
    routes::auth::VERIFICATION_EXPIRATION_DAYS,
    utils::{get_connection, send_email},
    AppState,
};
use actix_web::{error::ErrorConflict, put, web, HttpResponse, Responder};
use askama::Template;
use db_connector::models::users::User;
use diesel::{result::Error::NotFound, ExpressionMethods, QueryDsl, SelectableHelper};
use diesel_async::RunQueryDsl as _;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use validator::Validate;

#[allow(unused)]
#[derive(Template)]
#[template(path = "email_change_notification_en.html")]
struct EmailChangeNotificationEn {
    name: String,
    sender_email: String,
    brand: branding::Brand,
}

#[allow(unused)]
#[derive(Template)]
#[template(path = "email_change_notification_de.html")]
struct EmailChangeNotificationDe {
    name: String,
    sender_email: String,
    brand: branding::Brand,
}

#[allow(unused)]
fn send_email_change_notification(
    name: String,
    old_email: String,
    lang: String,
    state: web::Data<AppState>,
) {
    let (body, subject) = match lang.as_str() {
        "de" => {
            let template = EmailChangeNotificationDe {
                name: name.to_string(),
                sender_email: state.sender_email.clone(),
                brand: state.brand,
            };
            match template.render() {
                Ok(body) => (body, "E-Mail-Adresse geändert"),
                Err(e) => {
                    log::error!("Failed to render German email change notification template for user '{name}': {e}");
                    return;
                }
            }
        }
        _ => {
            let template = EmailChangeNotificationEn {
                name: name.to_string(),
                sender_email: state.sender_email.clone(),
                brand: state.brand,
            };
            match template.render() {
                Ok(body) => (body, "Email address changed"),
                Err(e) => {
                    log::error!("Failed to render English email change notification template for user '{name}': {e}");
                    return;
                }
            }
        }
    };

    log::info!("Sending email change notification to '{old_email}' for user '{name}'");
    send_email(&old_email, subject, body, &state);
}

#[allow(unused)]
fn send_verification_mail(
    name: String,
    email: String,
    lang: String,
    state: web::Data<AppState>,
    verification_id: uuid::Uuid,
) {
    let (body, subject) = match lang.as_str() {
        "de" => {
            let template = crate::routes::auth::register::VerifyEmailDETemplate {
                name: &name,
                link: &format!(
                    "{}/api/auth/verify?id={}",
                    state.frontend_url, verification_id
                ),
                brand: state.brand,
            };
            match template.render() {
                Ok(body) => (body, "E-Mail-Adresse bestätigen"),
                Err(e) => {
                    log::error!(
                            "Failed to render German verification email template for user '{name}': {e}"
                        );
                    return;
                }
            }
        }
        _ => {
            let template = crate::routes::auth::register::VerifyEmailENTemplate {
                name: &name,
                link: &format!(
                    "{}/api/auth/verify?id={}",
                    state.frontend_url, verification_id
                ),
                brand: state.brand,
            };
            match template.render() {
                Ok(body) => (body, "Verify email address"),
                Err(e) => {
                    log::error!("Failed to render English verification email template for user '{name}': {e}");
                    return;
                }
            }
        }
    };

    log::info!("Sending verification email to '{email}' for user '{name}'");
    send_email(&email, subject, body, &state);
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
    {
        let mut conn = get_connection(&state).await?;
        match users
            .filter(email.eq(&user_cpy.email.to_lowercase()))
            .select(User::as_select())
            .get_result::<User>(&mut conn)
            .await
        {
            Err(NotFound) => {}
            Ok(u) => {
                if u.id != uid {
                    return Err(Error::UserAlreadyExists.into());
                }
            }
            Err(_err) => return Err(Error::InternalError.into()),
        }
    }

    let old_user: User = {
        let mut conn = get_connection(&state).await?;
        match users
            .find::<uuid::Uuid>(uid)
            .select(User::as_select())
            .get_result::<User>(&mut conn)
            .await
        {
            Ok(u) => u,
            Err(NotFound) => return Err(Error::Unauthorized.into()),
            Err(_err) => return Err(Error::InternalError.into()),
        }
    };

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

    {
        let mut conn = get_connection(&state).await?;

        // Update user fields
        let updated = diesel::update(users.find::<uuid::Uuid>(uid))
            .set((
                name.eq(&new_user.name),
                email.eq(&new_user.email.to_lowercase()),
                delivery_email.eq(&new_user.email),
                email_verified.eq(new_user.email == old_user.email),
                old_email.eq(&old_user.email),
                old_delivery_email.eq(&old_user.delivery_email),
            ))
            .execute(&mut conn)
            .await;
        match updated {
            Ok(_) => {}
            Err(NotFound) => return Err(Error::Unauthorized.into()),
            Err(_err) => return Err(Error::InternalError.into()),
        }
    }

    if let Some(exp) = exp {
        let verify = db_connector::models::verification::Verification {
            id: uuid::Uuid::new_v4(),
            user: uid,
            expiration: exp,
        };

        // Insert verification record
        {
            let mut conn = get_connection(&state).await?;
            use db_connector::schema::verification::dsl::*;
            diesel::insert_into(verification)
                .values(&verify)
                .execute(&mut conn)
                .await
                .map_err(|_| Error::InternalError)?;
        }

        #[cfg(not(test))]
        {
            let runtime_handle = tokio::runtime::Handle::current();
            let verification_name = new_user.name.clone();
            let verification_email = new_user.email.clone();
            let old_user_name = old_user.name.clone();
            let old_user_email = old_user
                .delivery_email
                .clone()
                .unwrap_or_else(|| old_user.email.clone());
            let verification_state = state.clone();
            let notification_state = state.clone();
            let verification_lang: String = lang.into();
            let notification_lang = verification_lang.clone();

            drop(runtime_handle.spawn_blocking(move || {
                send_verification_mail(
                    verification_name,
                    verification_email,
                    verification_lang,
                    verification_state,
                    verify.id,
                );
                send_email_change_notification(
                    old_user_name,
                    old_user_email,
                    notification_lang,
                    notification_state,
                );
            }));
        }
    }

    Ok(HttpResponse::Ok())
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use crate::{
        defer_async,
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
    use diesel_async::RunQueryDsl as _AsyncRunQueryDsl;

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
        let mail_owned = mail.to_string();
        let key = create_user(mail).await;
        defer_async!({
            let inner_mail = mail_owned.clone();
            async move { delete_user(&inner_mail).await }
        });
        let update_mail = format!("t{mail}");
        defer_async!({
            let inner_mail = update_mail.clone();
            async move { delete_user(&inner_mail).await }
        });

        let app = App::new()
            .configure(configure)
            .service(update_user)
            .wrap(crate::middleware::jwt::JwtMiddleware);
        let app = test::init_service(app).await;

        let user = get_test_user(mail).await;
        let old_user = user.clone();
        let new_email = update_mail.clone();
        let user_schema = UpdateUserSchema {
            name: user.name.clone(),
            email: new_email.clone(),
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
        let updated_user = get_test_user(&update_mail).await;
        assert!(!updated_user.email_verified);
        assert_eq!(old_user.email, updated_user.old_email.unwrap());
        assert_eq!(old_user.delivery_email, updated_user.old_delivery_email);

        let pool = test_connection_pool();
        let mut conn = pool.get().await.unwrap();
        let user_id = updated_user.id;
        // Check that verification record was created
        use db_connector::schema::verification::dsl as verification_dsl;
        let verify_record = verification_dsl::verification
            .filter(verification_dsl::user.eq(user_id))
            .select(db_connector::models::verification::Verification::as_select())
            .get_result::<db_connector::models::verification::Verification>(&mut conn)
            .await
            .unwrap();
        assert!(verify_record.expiration > chrono::Utc::now().naive_utc());
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

        let user = get_test_user(&mail).await;
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
        let db_user = get_test_user(&mail).await;
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
        let updated_user = get_test_user(&mail).await;
        assert!(updated_user.email_verified);

        let pool = test_connection_pool();
        let mut conn = pool.get().await.unwrap();
        let user_id = updated_user.id;
        // Verify no verification record was created
        use db_connector::schema::verification::dsl as verification_dsl;
        let verify_records = verification_dsl::verification
            .filter(verification_dsl::user.eq(user_id))
            .select(db_connector::models::verification::Verification::as_select())
            .load::<db_connector::models::verification::Verification>(&mut conn)
            .await
            .unwrap();
        assert!(verify_records.is_empty());
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
        let db_user = get_test_user(&mail).await;
        let new_email = format!("changed_{mail}");
        let update = UpdateUserSchema {
            name: db_user.name.clone(),
            email: new_email.clone(),
        };

        defer_async!({
            let inner_email = new_email;
            async move { delete_user(&inner_email).await }
        });

        let req = test::TestRequest::put()
            .uri("/update_user")
            .set_json(update)
            .cookie(Cookie::new("access_token", token))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_success());

        // Attempt second email change while first is pending
        let another_email = format!("another_{mail}");
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
