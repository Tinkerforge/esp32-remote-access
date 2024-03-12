pub mod auth;
pub mod charger;
pub mod management;
pub mod user;

use actix_web::web::{self, scope};

use crate::{middleware::jwt::JwtMiddleware, ws_udp_bridge};

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.configure(user::configure);
    cfg.configure(auth::configure);
    cfg.configure(charger::configure);

    let scope = scope("")
        .wrap(JwtMiddleware)
        .service(management::management)
        .service(ws_udp_bridge::start_ws);

    cfg.service(scope);
}
