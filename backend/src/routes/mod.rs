pub(crate) mod auth;
mod user;
mod wallbox;

use actix_web::web;

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.configure(user::configure);
    cfg.configure(auth::configure);
}
