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

use std::num::NonZeroU32;

use actix_governor::{KeyExtractor, SimpleKeyExtractionError};
use actix_web::{http::StatusCode, HttpRequest, HttpResponse, ResponseError};
use governor::{
    clock::{Clock, QuantaClock, QuantaInstant},
    state::InMemoryState,
    NotUntil, Quota, RateLimiter,
};

/**
 * The struct used to extract ip for ratelimiting
 */
#[derive(Clone)]
pub struct IPExtractor;

impl KeyExtractor for IPExtractor {
    type Key = String;
    type KeyExtractionError = SimpleKeyExtractionError<&'static str>;

    fn extract(
        &self,
        req: &actix_web::dev::ServiceRequest,
    ) -> Result<Self::Key, Self::KeyExtractionError> {
        let info = req.connection_info();
        if let Some(ip) = info.realip_remote_addr() {
            Ok(ip.to_string())
        } else {
            Err(SimpleKeyExtractionError::new("Invalid real IP"))
        }
    }

    fn name(&self) -> &'static str {
        "IPExtractor"
    }
}

impl IPExtractor {
    pub fn new() -> Self {
        Self
    }
}

fn ip_from_req(req: &HttpRequest) -> actix_web::Result<String> {
    let ip = if let Some(ip) = req.connection_info().realip_remote_addr() {
        ip.to_string()
    } else {
        println!("No ip found for route {}", req.path());
        return Err(crate::error::Error::InternalError.into());
    };

    Ok(ip)
}

#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct LoginRateLimitKey {
    user: String,
    ip: String,
}

#[cfg(test)]
const REQUESTS_PER_SECOND: u32 = 1;

#[cfg(test)]
const REQUESTS_BURST: u32 = 5;

#[cfg(not(test))]
const REQUESTS_PER_SECOND: u32 = 5;

#[cfg(not(test))]
const REQUESTS_BURST: u32 = 25;

// RateLimiter for the login route
pub struct LoginRateLimiter(
    RateLimiter<
        LoginRateLimitKey,
        dashmap::DashMap<LoginRateLimitKey, InMemoryState>,
        QuantaClock,
        governor::middleware::NoOpMiddleware<governor::clock::QuantaInstant>,
    >,
);

impl LoginRateLimiter {
    pub fn new() -> Self {
        Self(RateLimiter::keyed(
            Quota::per_second(NonZeroU32::new(REQUESTS_PER_SECOND).unwrap())
                .allow_burst(NonZeroU32::new(REQUESTS_BURST).unwrap()),
        ))
    }

    pub fn check(&self, email: String, req: &HttpRequest) -> actix_web::Result<()> {
        let ip = ip_from_req(req)?;

        let key = LoginRateLimitKey { user: email, ip };
        if let Err(err) = self.0.check_key(&key) {
            log::warn!("RateLimiter triggered for {:?}", key);
            let now = self.0.clock().now();

            Err(RateLimitError::new(err, now).into())
        } else {
            Ok(())
        }
    }
}

#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct ChargerRateLimitKey {
    charger_id: String,
    ip: String,
}

// Rate limiter for all routes that get called by chargers
pub struct ChargerRateLimiter(
    RateLimiter<
        ChargerRateLimitKey,
        dashmap::DashMap<ChargerRateLimitKey, InMemoryState>,
        QuantaClock,
        governor::middleware::NoOpMiddleware<governor::clock::QuantaInstant>,
    >,
);

impl ChargerRateLimiter {
    pub fn new() -> Self {
        Self(RateLimiter::keyed(
            Quota::per_second(NonZeroU32::new(REQUESTS_PER_SECOND).unwrap())
                .allow_burst(NonZeroU32::new(REQUESTS_BURST).unwrap()),
        ))
    }

    pub fn check(&self, charger_id: String, req: &HttpRequest) -> actix_web::Result<()> {
        let ip = ip_from_req(req)?;

        let key = ChargerRateLimitKey { charger_id, ip };
        if let Err(err) = self.0.check_key(&key) {
            log::warn!("RateLimiter triggered for {:?}", key);
            let now = self.0.clock().now();

            Err(RateLimitError::new(err, now).into())
        } else {
            Ok(())
        }
    }
}

#[derive(Debug)]
struct RateLimitError {
    wait_time: NotUntil<QuantaInstant>,
    now: QuantaInstant,
}

impl RateLimitError {
    pub fn new(wait_time: NotUntil<QuantaInstant>, now: QuantaInstant) -> Self {
        Self { wait_time, now }
    }
}

impl std::fmt::Display for RateLimitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let wait_time = self.wait_time.wait_time_from(self.now);
        write!(f, "Retry in {} seconds.", wait_time.as_secs())
    }
}

impl ResponseError for RateLimitError {
    fn status_code(&self) -> actix_web::http::StatusCode {
        StatusCode::TOO_MANY_REQUESTS
    }

    fn error_response(&self) -> HttpResponse<actix_web::body::BoxBody> {
        let wait_time = self.wait_time.wait_time_from(self.now);
        HttpResponse::TooManyRequests()
            .append_header(("retry-after", wait_time.as_secs()))
            .append_header(("x-retry-after", wait_time.as_secs()))
            .body(self.to_string())
    }
}

#[cfg(test)]
mod tests {
    use actix_web::test;

    use crate::rate_limit::ChargerRateLimiter;

    use super::LoginRateLimiter;

    #[actix_web::test]
    async fn test_login_rate_limiter() {
        let limiter = LoginRateLimiter::new();
        let req = test::TestRequest::get()
            .uri("/login")
            .insert_header(("X-Forwarded-For", "123.123.123.2"))
            .to_http_request();
        let email = "abc@de.fg".to_string();

        let ret = limiter.check(email.clone(), &req);
        assert!(ret.is_ok());

        let ret = limiter.check(email.clone(), &req);
        assert!(ret.is_ok());

        let ret = limiter.check(email.clone(), &req);
        assert!(ret.is_ok());

        let ret = limiter.check(email.clone(), &req);
        assert!(ret.is_ok());

        let ret = limiter.check(email.clone(), &req);
        assert!(ret.is_ok());

        let ret = limiter.check(email.clone(), &req);
        assert!(ret.is_err());

        let email2 = "gf@edc.ba".to_string();
        let ret = limiter.check(email2.clone(), &req);
        assert!(ret.is_ok());

        let req = test::TestRequest::get()
            .uri("/login")
            .insert_header(("X-Forwarded-For", "123.123.123.3"))
            .to_http_request();
        let ret = limiter.check(email.clone(), &req);
        assert!(ret.is_ok());
    }

    #[actix_web::test]
    async fn test_charger_rate_limiter() {
        let limiter = ChargerRateLimiter::new();
        let req = test::TestRequest::get()
            .uri("/login")
            .insert_header(("X-Forwarded-For", "123.123.123.2"))
            .to_http_request();
        let email = uuid::Uuid::new_v4().to_string();

        let ret = limiter.check(email.clone(), &req);
        assert!(ret.is_ok());

        let ret = limiter.check(email.clone(), &req);
        assert!(ret.is_ok());

        let ret = limiter.check(email.clone(), &req);
        assert!(ret.is_ok());

        let ret = limiter.check(email.clone(), &req);
        assert!(ret.is_ok());

        let ret = limiter.check(email.clone(), &req);
        assert!(ret.is_ok());

        let ret = limiter.check(email.clone(), &req);
        assert!(ret.is_err());

        let email2 = uuid::Uuid::new_v4().to_string();
        let ret = limiter.check(email2.clone(), &req);
        assert!(ret.is_ok());

        let req = test::TestRequest::get()
            .uri("/login")
            .insert_header(("X-Forwarded-For", "123.123.123.3"))
            .to_http_request();
        let ret = limiter.check(email.clone(), &req);
        assert!(ret.is_ok());
    }
}
