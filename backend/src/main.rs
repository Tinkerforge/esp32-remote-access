mod routes;
mod models;
mod middleware;

use actix_web::{web, App, HttpServer};
use db_connector::*;
use routes::register_routes;

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    dotenv::dotenv().ok();

    let pool = get_connection_pool();
    let mut conn = pool.get().expect("Failed to get connection from pool");
    run_migrations(&mut conn).expect("Failed to run migrations");

    let state = web::Data::new(AppState {
        pool,
        jwt_secret: std::env::var("JWT_SECRET").expect("JWT_SECRET must be set!")
    });

    HttpServer::new(move || {
        let cors = actix_cors::Cors::permissive();
        let app = App::new()
            .wrap(cors)
            .app_data(state.clone());

        register_routes(app)
    })
    .bind("0.0.0.0:8081")?
    .run()
    .await
}

pub struct AppState {
    pub pool: Pool,
    pub jwt_secret: String
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use actix_web::web::ServiceConfig;

    pub struct ScopeCall<F: FnMut()> {
        pub c: F
    }
    impl <F: FnMut()> Drop for ScopeCall<F> {
        fn drop(&mut self) {
            (self.c)();
        }
    }

    #[macro_export]
    macro_rules! defer {
        ($e:expr) => {
            let _scope_call = crate::tests::ScopeCall { c: || -> () { $e; }};
        };
    }

    pub fn configure(cfg: &mut ServiceConfig) {
        let pool = db_connector::test_connection_pool();
        let state = AppState {
            pool,
            jwt_secret: std::env::var("JWT_SECRET").expect("JWT_SECRET must be set!")
        };
        let state = web::Data::new(state);
        cfg.app_data(state.clone());
    }
}
