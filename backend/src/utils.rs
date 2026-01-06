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

use actix_web::web;
use db_connector::models::authorization_tokens::AuthorizationToken;
use db_connector::models::chargers::Charger;
use diesel::prelude::*;
use diesel::{
    r2d2::{ConnectionManager, PooledConnection},
    result::Error::NotFound,
    PgConnection,
};
#[cfg(not(test))]
use lettre::message::header::ContentType;
#[cfg(not(test))]
use lettre::{Message, Transport};
use rand::Rng;

use crate::{error::Error, routes::charger::add::password_matches, AppState};

pub fn get_connection(
    state: &web::Data<AppState>,
) -> actix_web::Result<PooledConnection<ConnectionManager<PgConnection>>> {
    match state.pool.get() {
        Ok(conn) => Ok(conn),
        Err(err) => {
            log::error!("Failed to get database connection: {err}");
            Err(Error::InternalError.into())
        }
    }
}

pub fn generate_random_bytes() -> Vec<u8> {
    let mut rng = rand::rng();
    (0..24).map(|_| rng.random_range(0..255)).collect()
}

pub async fn web_block_unpacked<F, R>(f: F) -> Result<R, actix_web::Error>
where
    F: FnOnce() -> Result<R, Error> + Send + 'static,
    R: Send + 'static,
{
    match web::block(f).await {
        Ok(res) => match res {
            Ok(v) => Ok(v),
            Err(err) => Err(err.into()),
        },
        Err(_err) => Err(Error::InternalError.into()),
    }
}

pub fn as_u8_slice<T: Sized>(p: &T) -> &[u8] {
    unsafe {
        ::core::slice::from_raw_parts((p as *const T) as *const u8, ::core::mem::size_of::<T>())
    }
}

pub fn parse_uuid(uuid: &str) -> actix_web::Result<uuid::Uuid> {
    match uuid::Uuid::from_str(uuid) {
        Ok(v) => Ok(v),
        Err(err) => Err(actix_web::error::ErrorBadRequest(err)),
    }
}

pub async fn get_charger_by_uid(
    uid: i32,
    password: Option<String>,
    state: &web::Data<AppState>,
) -> actix_web::Result<Charger> {
    let password = if let Some(password) = password {
        password
    } else {
        return Err(actix_web::error::ErrorBadRequest("Password is missing"));
    };

    let mut conn = get_connection(state)?;
    let chargers: Vec<Charger> = web_block_unpacked(move || {
        use db_connector::schema::chargers::dsl as chargers;

        match chargers::chargers
            .filter(chargers::uid.eq(uid))
            .select(Charger::as_select())
            .load(&mut conn)
        {
            Ok(c) => Ok(c),
            Err(NotFound) => {
                println!("C");
                Err(Error::ChargerCredentialsWrong)
            }
            Err(_err) => Err(Error::InternalError),
        }
    })
    .await?;

    for c in chargers.into_iter() {
        println!("D");
        if password_matches(&password, &c.password)? {
            return Ok(c);
        }
    }

    Err(Error::ChargerCredentialsWrong.into())
}

pub async fn get_charger_from_db(
    charger_id: uuid::Uuid,
    state: &web::Data<AppState>,
) -> actix_web::Result<Charger> {
    let mut conn = get_connection(state)?;
    let charger: Charger = web_block_unpacked(move || {
        use db_connector::schema::chargers::dsl::*;

        match chargers
            .filter(id.eq(charger_id))
            .select(Charger::as_select())
            .get_result(&mut conn)
        {
            Ok(c) => Ok(c),
            Err(NotFound) => Err(Error::WrongCredentials),
            Err(_err) => Err(Error::InternalError),
        }
    })
    .await?;

    Ok(charger)
}

pub async fn validate_auth_token(
    token: String,
    user_id: uuid::Uuid,
    state: &web::Data<AppState>,
) -> actix_web::Result<()> {
    let mut conn = get_connection(state)?;
    let token: AuthorizationToken = web_block_unpacked(move || {
        use db_connector::schema::authorization_tokens::dsl as authorization_tokens;

        match authorization_tokens::authorization_tokens
            .filter(authorization_tokens::user_id.eq(user_id))
            .filter(authorization_tokens::token.eq(token))
            .select(AuthorizationToken::as_select())
            .get_result(&mut conn)
        {
            Ok(v) => Ok(v),
            Err(NotFound) => Err(Error::AuthorizationTokenInvalid),
            Err(_err) => Err(Error::InternalError),
        }
    })
    .await?;

    if token.use_once && token.last_used_at.is_some() {
        return Err(Error::AuthorizationTokenAlreadyUsed.into());
    }

    let mut conn = get_connection(state)?;
    web_block_unpacked(move || {
        use db_connector::schema::authorization_tokens::dsl as authorization_tokens;

        match diesel::update(authorization_tokens::authorization_tokens)
            .filter(authorization_tokens::id.eq(token.id))
            .set(authorization_tokens::last_used_at.eq(chrono::Utc::now().naive_utc()))
            .execute(&mut conn)
        {
            Ok(_) => Ok(()),
            Err(_err) => Err(Error::InternalError),
        }
    })
    .await?;

    Ok(())
}

pub fn send_email(email: &str, subject: &str, body: String, state: &web::Data<AppState>) {
    #[cfg(not(test))]
    {
        if let Some(ref mailer) = state.mailer {
            let email = Message::builder()
                .from(
                    format!("{} <{}>", state.sender_name, state.sender_email)
                        .parse()
                        .unwrap(),
                )
                .to(email.parse().unwrap())
                .subject(subject)
                .header(ContentType::TEXT_HTML)
                .body(body)
                .unwrap();

            match mailer.send(&email) {
                Ok(_) => log::info!("Email sent successfully!"),
                Err(e) => log::error!("Could not send email: {e:?}"),
            }
        } else {
            log::error!("No mailer configured, email not sent");
        }
    }

    #[cfg(test)]
    {
        let _ = body;
        let _ = state;
        println!("Test mode: Email would be sent to {email} with subject '{subject}'");
    }
}

/// Send an email with a binary attachment (chargelog)
pub fn send_email_with_attachment(
    email: &str,
    subject: &str,
    body: String,
    attachment_data: Vec<u8>,
    attachment_filename: &str,
    state: &web::Data<AppState>,
) {
    #[cfg(not(test))]
    {
        if let Some(ref mailer) = state.mailer {
            let multipart = lettre::message::MultiPart::mixed()
                .singlepart(
                    lettre::message::SinglePart::builder()
                        .header(lettre::message::header::ContentType::TEXT_HTML)
                        .body(body),
                )
                .singlepart(
                    lettre::message::Attachment::new(attachment_filename.to_string()).body(
                        attachment_data,
                        lettre::message::header::ContentType::parse("application/octet-stream")
                            .unwrap(),
                    ),
                );

            let email = lettre::Message::builder()
                .from(
                    format!("{} <{}>", state.sender_name, state.sender_email)
                        .parse()
                        .unwrap(),
                )
                .to(email.parse().unwrap())
                .subject(subject)
                .multipart(multipart)
                .unwrap();

            match mailer.send(&email) {
                Ok(_) => log::info!("Email with attachment sent successfully!"),
                Err(e) => log::error!("Could not send email: {e:?}"),
            }
        } else {
            log::error!("No mailer configured, email not sent");
        }
    }

    #[cfg(test)]
    {
        let _ = body;
        let _ = state;
        let _ = attachment_data;
        let _ = attachment_filename;
        println!(
            "Test mode: Email would be sent to {email} with subject '{subject}' and attachment {attachment_filename}"
        );
    }
}

pub async fn update_charger_state_change(charger_id: uuid::Uuid, state: web::Data<AppState>) {
    let Ok(mut conn) = get_connection(&state) else {
        log::error!("Failed to get database connection for updating charger state change");
        return;
    };
    let _ = web_block_unpacked(move || {
        use db_connector::schema::chargers::dsl::*;

        match diesel::update(chargers)
            .filter(id.eq(charger_id))
            .set(last_state_change.eq(Some(chrono::Utc::now().naive_utc())))
            .execute(&mut conn)
        {
            Ok(_) => Ok(()),
            Err(_err) => {
                log::error!("Failed to update last_state_change for charger {charger_id}: {_err}");
                Err(Error::InternalError)
            }
        }
    })
    .await;
}

#[cfg(test)]
mod tests {
    use crate::{
        routes::user::tests::{get_test_uuid, TestUser},
        tests::create_test_state,
        utils::validate_auth_token,
    };

    #[actix_web::test]
    async fn test_validate_auth_token() {
        let (mut user, mail) = TestUser::random().await;
        user.login().await;
        let token = user.create_authorization_token(true).await;
        let user_id = get_test_uuid(&mail).unwrap();
        let state = create_test_state(None);
        assert!(validate_auth_token(token.token.clone(), user_id, &state)
            .await
            .is_ok());
        assert!(validate_auth_token(token.token, user_id, &state)
            .await
            .is_err());

        let token = user.create_authorization_token(false).await;
        assert!(validate_auth_token(token.token.clone(), user_id, &state)
            .await
            .is_ok());
        assert!(validate_auth_token(token.token, user_id, &state)
            .await
            .is_ok());
    }
}
