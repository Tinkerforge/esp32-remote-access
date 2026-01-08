use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Serialize, Deserialize, ToSchema, Clone, PartialEq, Debug, Ord, PartialOrd, Eq)]
pub struct ResponseAuthorizationToken {
    pub id: String,
    pub token: String,
    pub use_once: bool,
    pub name: String,
    pub created_at: i64,
    pub last_used_at: Option<i64>,
}
