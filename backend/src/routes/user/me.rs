use actix_web::{get, web, HttpMessage, HttpRequest, HttpResponse, Responder};

use crate::{error::Error, models::filtered_user::FilteredUser, routes::user::get_user, AppState};

#[get("/me")]
async fn me(req: HttpRequest, state: web::Data<AppState>) -> Result<impl Responder, actix_web::Error> {
    let ext = req.extensions();
    let user_id = match ext.get::<uuid::Uuid>() {
        Some(uid) => uid.to_owned(),
        None => {
            return Err(Error::InternalError.into())
        }
    };

    let user = get_user(&state, user_id).await?;

    Ok(HttpResponse::Ok().json(FilteredUser::from(user)))
}
