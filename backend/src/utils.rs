use actix_web::web;
use diesel::{
    r2d2::{ConnectionManager, PooledConnection},
    PgConnection,
};

use crate::{error::Error, AppState};

pub fn get_connection(
    state: &web::Data<AppState>,
) -> actix_web::Result<PooledConnection<ConnectionManager<PgConnection>>> {
    match state.pool.get() {
        Ok(conn) => Ok(conn),
        Err(_err) => Err(Error::InternalError.into()),
    }
}

pub async fn web_block_unpacked<F, R>(f: F) -> Result<R, actix_web::Error>
where
    F: FnOnce() -> Result<R, Error> + Send + 'static,
    R: Send + 'static,
{
    match web::block(f).await {
        Ok(res) => match res {
            Ok(v) => Ok(v),
            Err(err) => Err(err.into()),
        },
        Err(_err) => Err(Error::InternalError.into()),
    }
}

pub fn as_u8_slice<T: Sized>(p: &T) -> &[u8] {
    unsafe {
        ::core::slice::from_raw_parts((p as *const T) as *const u8, ::core::mem::size_of::<T>())
    }
}
