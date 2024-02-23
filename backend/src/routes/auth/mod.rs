use actix_web::web::{self, ServiceConfig};

pub mod login;
pub mod logout;
pub mod register;
pub mod verify;

pub fn configure(cfg: &mut ServiceConfig) {
    let scope = web::scope("/auth")
        .service(register::register)
        .service(verify::verify)
        .service(logout::logout)
        .service(login::login);
    cfg.service(scope);
}
