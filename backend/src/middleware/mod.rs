use actix_web::{http, HttpRequest};

pub mod jwt;
pub mod jwt_refresh;

pub fn get_token(req: &HttpRequest, name: &str) -> Option<String> {
    req
        .cookie(name)
        .map(|c| c.value().to_string())
        .or_else(|| {
            req.headers()
                .get(http::header::AUTHORIZATION)
                .map(|h| h.to_str().unwrap().split_at(7).1.to_string())
        })
}
