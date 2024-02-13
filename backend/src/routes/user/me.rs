use actix_web::{get, web, HttpMessage, HttpRequest, HttpResponse, Responder};
use db_connector::models::users::User;
use diesel::prelude::*;

use crate::AppState;



#[get("/me")]
async fn me(req: HttpRequest, state: web::Data<AppState>) -> impl Responder {
    use db_connector::schema::users::dsl::*;

    let mut conn = match state.pool.get() {
        Ok(conn) => conn,
        Err(_err) => {
            return HttpResponse::InternalServerError()
        }
    };

    let ext = req.extensions();
    let user_id = match ext.get::<uuid::Uuid>() {
        Some(uid) => uid.to_owned(),
        None => {
            return HttpResponse::InternalServerError()
        }
    };

    let result = web::block(move || {
        users.filter(id.eq(user_id))
            .select(User::as_select())
            .load(&mut conn)
    }).await.unwrap();

    let user: User = match result {
        Ok(user) => {
            if user.len() == 1 {
                user[0].clone()
            } else {
                return HttpResponse::BadRequest()
            }
        },
        Err(_err) => {
            return HttpResponse::InternalServerError()
        }
    };

    HttpResponse::Ok()
}
