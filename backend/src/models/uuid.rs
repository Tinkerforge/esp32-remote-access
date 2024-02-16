use std::future::{ready, Ready};

use actix_web::HttpMessage;

#[derive(Clone, Debug)]
pub struct Uuid(pub uuid::Uuid);

impl Uuid {
    pub fn new(id: uuid::Uuid) -> Self {
        Self(id)
    }
}

impl Into<uuid::Uuid> for Uuid {
    fn into(self) -> uuid::Uuid {
        self.0
    }
}

impl actix_web::FromRequest for Uuid {
    type Error = actix_web::Error;
    type Future = Ready<Result<Self, Self::Error>>;

    fn from_request(req: &actix_web::HttpRequest, _: &mut actix_web::dev::Payload) -> Self::Future {
        let ext = req.extensions();
        ready(Ok(Self::new(*ext.get::<uuid::Uuid>().unwrap())))
    }
}
