use actix_web::web::{self, ServiceConfig};

pub(crate) mod login;
mod logout;
pub(crate) mod register;
pub(crate) mod verify;

pub fn configure(cfg: &mut ServiceConfig) {
    let scope = web::scope("/auth")
        .service(register::register)
        .service(verify::verify)
        .service(logout::logout)
        .service(login::login);
    cfg.service(scope);
}
