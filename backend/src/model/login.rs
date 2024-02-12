use serde::{Deserialize, Serialize};
use validator::Validate;


#[derive(Serialize, Deserialize, Clone, Debug, Validate)]
pub struct LoginSchema {
    pub email: String,
    pub password: String
}
