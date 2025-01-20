use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Serialize, Deserialize, ToSchema, Clone, PartialEq, Debug)]
pub struct ResponseAuthorizationToken {
    pub id: String,
    pub token: String,
    pub use_once: bool,
}
