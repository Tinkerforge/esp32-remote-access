/* esp32-remote-access
 * Copyright (C) 2025 Frederic Henrichs <frederic@tinkerforge.com>
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

pub mod auth;
pub mod charger;
pub mod check_expiration;
pub mod grouping;
pub mod management;
pub mod selfdestruct;
pub mod send_chargelog_to_user;
pub mod state;
pub mod user;
pub mod webinterface;

use actix_web::web::{self, scope};

use crate::{middleware::jwt::JwtMiddleware, ws_udp_bridge};

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.configure(user::configure);
    cfg.configure(auth::configure);
    cfg.configure(charger::configure);
    cfg.configure(grouping::configure);

    cfg.service(management::management);
    cfg.service(send_chargelog_to_user::send_chargelog);
    cfg.service(selfdestruct::selfdestruct);
    cfg.service(check_expiration::check_expiration);

    // #[cfg(debug_assertions)]
    cfg.service(state::state);

    let scope = scope("")
        .wrap(JwtMiddleware)
        .service(ws_udp_bridge::start_ws)
        .service(webinterface::get_webinterface);
    cfg.service(scope);
}

#[cfg(test)]
mod tests {
    use actix_web::{test, web, App};

    use crate::tests::{call_service, configure as configure_test_state};

    /// The `/api/state` endpoint dumps the contents of the in-memory
    /// `BridgeState` and is therefore only intended for development and
    /// debugging. It must not be reachable in production (release) builds.
    ///
    /// `routes::configure` only registers the service when
    /// `debug_assertions` is enabled, so the endpoint exists in `cargo test`
    /// (debug) builds and is absent in `cargo test --release`
    /// (production-like) builds. This test asserts both halves of that
    /// contract so a regression that drops the `cfg` guard is caught no
    /// matter which profile CI exercises.
    #[actix_web::test]
    async fn state_endpoint_is_debug_only() {
        // Reuse the production route tree. In release builds the state
        // service is not registered, so a request to `/api/state` falls
        // through to the catch-all JWT-protected scope, which rejects it
        // with 401. Any 2xx answer therefore proves it was actually
        // exposed.
        let app = App::new()
            .service(web::scope("/api").configure(super::configure))
            .configure(configure_test_state);
        let app = test::init_service(app).await;

        let req = test::TestRequest::get().uri("/api/state").to_request();
        let resp = call_service(&app, req).await;

        if cfg!(debug_assertions) {
            assert!(
                resp.status().is_success(),
                "state endpoint must be exposed in debug builds, got {}",
                resp.status(),
            );
        } else {
            assert_ne!(
                resp.status().as_u16(),
                200,
                "state endpoint must not be exposed in production (release) builds",
            );
            assert_eq!(
                resp.status().as_u16(),
                401,
                "state endpoint must not be exposed in production (release) builds \
                 (request must fall through to the JWT-protected catch-all scope and \
                 be rejected as unauthorized, got {})",
                resp.status(),
            );
        }
    }
}
