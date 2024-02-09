mod routes;
mod model;

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
        pool
    });

    HttpServer::new(move || {
        let app = App::new()
            .app_data(state.clone());

        register_routes(app)

    })
    .bind("0.0.0.0:8081")?
    .run()
    .await
}

pub struct AppState {
    pub pool: Pool
}
