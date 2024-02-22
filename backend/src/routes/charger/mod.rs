mod add;

use crate::middleware::jwt::JwtMiddleware;
use actix_web::web;

pub fn configure(cfg: &mut web::ServiceConfig) {
    let scope = web::scope("/wallbox").wrap(JwtMiddleware).service(add::add);
    cfg.service(scope);
}
