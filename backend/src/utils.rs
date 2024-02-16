use actix_web::{error::ErrorInternalServerError, web};
use diesel::{r2d2::{ConnectionManager, PooledConnection}, PgConnection};

use crate::AppState;


pub fn get_connection(state: &web::Data<AppState>) -> actix_web::Result<PooledConnection<ConnectionManager<PgConnection>>>
{
    match state.pool.get() {
        Ok(conn) => Ok(conn),
        Err(_err) => {
            Err(ErrorInternalServerError(""))
        }
    }
}
