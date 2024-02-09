mod auth;

use actix_web::{dev::{ServiceFactory, ServiceRequest}, App, Error};


pub fn register_routes<T>(app: App<T>) -> App<T>
where
    T: ServiceFactory<ServiceRequest, Config = (), Error = Error, InitError = ()> {
    auth::register_auth_routes(app)
}
