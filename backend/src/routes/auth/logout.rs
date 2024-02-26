use actix_web::{
    cookie::{time::Duration, Cookie},
    get, HttpResponse, Responder,
};

/// Logout user
#[utoipa::path(
    context_path = "/auth",
    responses(
        (status = 200, description = "User logged out")
    )
)]
#[get("/logout")]
pub async fn logout() -> impl Responder {
    let cookie = Cookie::build("access_token", "")
        .path("/")
        .max_age(Duration::new(-1, 0))
        .http_only(true)
        .finish();

    HttpResponse::Ok().cookie(cookie).body("")
}
