use actix_web::web::{self, ServiceConfig};

pub(crate) mod register;
pub(crate) mod login;
pub(crate) mod verify;
mod logout;

pub fn configure(cfg: &mut ServiceConfig) {
    let scope = web::scope("/auth")
        .service(register::register)
        .service(verify::verify)
        .service(logout::logout)
        .service(login::login);
    cfg.service(scope);
}
