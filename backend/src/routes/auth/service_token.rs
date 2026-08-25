/* esp32-remote-access
 * Copyright (C) 2026 Frederic Henrichs <frederic@tinkerforge.com>
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

use actix_web::{get, HttpResponse, Responder};

use crate::error::Error;

const SERVICE_AUTH_TOKEN_ENV: &str = "SERVICE_AUTH_TOKEN";

/// Return the pre-shared authorization token string configured via the
/// `SERVICE_AUTH_TOKEN` environment variable. Intended for service
/// integrations that need a long-lived charger authorization token without
/// going through the regular user login flow.
#[utoipa::path(
    context_path = "/auth",
    responses(
        (status = 200, description = "Service token was returned", body = String),
        (status = 500, description = "SERVICE_AUTH_TOKEN env var is missing"),
    )
)]
#[get("/service_token")]
pub async fn service_token() -> actix_web::Result<impl Responder> {
    let token = match std::env::var(SERVICE_AUTH_TOKEN_ENV) {
        Ok(value) if !value.is_empty() => value,
        Ok(_) | Err(_) => {
            log::error!("{SERVICE_AUTH_TOKEN_ENV} is not set; cannot serve service token");
            return Err(Error::InternalError.into());
        }
    };

    Ok(HttpResponse::Ok().body(token))
}

#[cfg(test)]
mod tests {
    use actix_web::{test, App};

    use super::service_token;

    #[actix_web::test]
    async fn test_returns_configured_token() {
        let expected = "my-secret-service-token";
        // SAFETY: this test sets a process-wide env var. Other tests that
        // touch the same env var must run sequentially to avoid observing
        // each other's values; we accept the documented unsafety of
        // set_var/remove_var for this scope.
        unsafe {
            std::env::set_var("SERVICE_AUTH_TOKEN", expected);
        }

        let app = App::new().service(service_token);
        let app = test::init_service(app).await;

        let req = test::TestRequest::get().uri("/service_token").to_request();
        let resp = test::call_service(&app, req).await;
        assert!(
            resp.status().is_success(),
            "expected 2xx, got {}",
            resp.status()
        );

        let body = test::read_body(resp).await;
        assert_eq!(body, expected.as_bytes());

        unsafe {
            std::env::remove_var("SERVICE_AUTH_TOKEN");
        }
    }

    #[actix_web::test]
    async fn test_missing_env_var_returns_500() {
        unsafe {
            std::env::remove_var("SERVICE_AUTH_TOKEN");
        }

        let app = App::new().service(service_token);
        let app = test::init_service(app).await;

        let req = test::TestRequest::get().uri("/service_token").to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 500);
    }

    #[actix_web::test]
    async fn test_empty_env_var_returns_500() {
        unsafe {
            std::env::set_var("SERVICE_AUTH_TOKEN", "");
        }

        let app = App::new().service(service_token);
        let app = test::init_service(app).await;

        let req = test::TestRequest::get().uri("/service_token").to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 500);

        unsafe {
            std::env::remove_var("SERVICE_AUTH_TOKEN");
        }
    }
}
