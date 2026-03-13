use actix_web::{http, HttpRequest};

pub mod jwt;

pub fn get_token(req: &HttpRequest, name: &str) -> Option<String> {
    req.cookie(name).map(|c| c.value().to_string()).or_else(|| {
        req.headers()
            .get(http::header::AUTHORIZATION)
            .and_then(|h| h.to_str().ok())
            .and_then(|h| h.strip_prefix("Bearer "))
            .map(|t| t.to_string())
    })
}

#[cfg(test)]
mod tests {
    use super::get_token;
    use actix_web::test::TestRequest;

    #[actix_web::test]
    async fn test_get_token_from_cookie() {
        let req = TestRequest::default()
            .cookie(actix_web::cookie::Cookie::new("access_token", "mytoken"))
            .to_http_request();
        assert_eq!(get_token(&req, "access_token"), Some("mytoken".to_string()));
    }

    #[actix_web::test]
    async fn test_get_token_from_bearer_header() {
        let req = TestRequest::default()
            .insert_header(("Authorization", "Bearer mytoken"))
            .to_http_request();
        assert_eq!(get_token(&req, "access_token"), Some("mytoken".to_string()));
    }

    #[actix_web::test]
    async fn test_get_token_rejects_non_bearer_auth() {
        let req = TestRequest::default()
            .insert_header(("Authorization", "Basic dXNlcjpwYXNz"))
            .to_http_request();
        assert_eq!(get_token(&req, "access_token"), None);
    }

    #[actix_web::test]
    async fn test_get_token_short_header_no_panic() {
        let req = TestRequest::default()
            .insert_header(("Authorization", "abc"))
            .to_http_request();
        assert_eq!(get_token(&req, "access_token"), None);
    }

    #[actix_web::test]
    async fn test_get_token_empty_header_no_panic() {
        let req = TestRequest::default()
            .insert_header(("Authorization", ""))
            .to_http_request();
        assert_eq!(get_token(&req, "access_token"), None);
    }

    #[actix_web::test]
    async fn test_get_token_cookie_takes_priority() {
        let req = TestRequest::default()
            .cookie(actix_web::cookie::Cookie::new("access_token", "cookie_token"))
            .insert_header(("Authorization", "Bearer header_token"))
            .to_http_request();
        assert_eq!(
            get_token(&req, "access_token"),
            Some("cookie_token".to_string())
        );
    }

    #[actix_web::test]
    async fn test_get_token_no_cookie_no_header() {
        let req = TestRequest::default().to_http_request();
        assert_eq!(get_token(&req, "access_token"), None);
    }
}
