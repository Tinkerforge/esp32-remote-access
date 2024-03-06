pub mod auth;
pub mod charger;
pub mod user;
pub mod management;

use actix_web::web::{self, scope};

use crate::middleware::jwt::JwtMiddleware;

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.configure(user::configure);
    cfg.configure(auth::configure);
    cfg.configure(charger::configure);

    let scope = scope("").wrap(JwtMiddleware)
        .service(management::management);

    cfg.service(scope);
}
