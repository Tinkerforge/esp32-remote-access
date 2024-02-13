use actix_web::{dev::{ServiceFactory, ServiceRequest}, web, App, Error};

use crate::middleware::jwt::JwtMiddleware;

mod me;

pub fn register_user_routes<T>(app: App<T>) -> App<T>
where
    T: ServiceFactory<ServiceRequest, Config = (), Error = Error, InitError = ()> {
    let scope = web::scope("/user").wrap(JwtMiddleware)
        .service(me::me);
    app.service(scope)
}
