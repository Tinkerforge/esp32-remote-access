use actix_web::{get, HttpResponse, Responder};

use crate::utils::generate_random_bytes;

/// Generate random bytes
// Need Vec<u32> here because otherwise it is recognised as String.
#[utoipa::path(
    context_path = "/auth",
    responses(
        (status = 200, body = Vec<u32>)
    )
)]
#[get("/generate_salt")]
pub async fn generate_salt() -> impl Responder {
    let salt = generate_random_bytes();
    HttpResponse::Ok().json(salt)
}
