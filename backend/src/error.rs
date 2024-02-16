use actix_web::{error, http::{header::ContentType, StatusCode}, HttpResponse};
use derive_more::{Display, Error};

#[derive(Debug, Display, Error)]
pub enum Error {
    #[display(fmt = "An internal error occured. Please try again later")]
    InternalError,
    #[display(fmt = "An account with this email already exists")]
    AlreadyExists,
    #[display(fmt = "Wrong username or password")]
    WrongCredentials,
    #[display(fmt = "Not verified")]
    NotVerified,
}

impl error::ResponseError for Error {
    fn error_response(&self) -> HttpResponse {
        HttpResponse::build(self.status_code())
            .insert_header(ContentType::plaintext())
            .body(self.to_string())
    }

    fn status_code(&self) -> StatusCode {
        match *self {
            Self::InternalError => StatusCode::INTERNAL_SERVER_ERROR,
            Self::AlreadyExists => StatusCode::CONFLICT,
            Self::WrongCredentials => StatusCode::BAD_REQUEST,
            Self::NotVerified => StatusCode::UNAUTHORIZED
        }
    }
}
