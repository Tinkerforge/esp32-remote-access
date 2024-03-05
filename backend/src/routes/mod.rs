pub mod auth;
pub mod charger;
pub mod user;
pub mod management;

use actix_web::web;

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.configure(user::configure);
    cfg.configure(auth::configure);
    cfg.configure(charger::configure);
    cfg.service(management::management);
}
