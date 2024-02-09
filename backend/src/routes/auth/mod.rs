use actix_web::{dev::{ServiceFactory, ServiceRequest}, web, App, Error};

mod register;


pub fn register_auth_routes<T>(app: App<T>) -> App<T>
where
    T: ServiceFactory<ServiceRequest, Config = (), Error = Error, InitError = ()> {
    let scope = web::scope("/auth")
        .service(register::register);
    app.service(scope)
}
