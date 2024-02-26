use db_connector::models::users::User;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use validator::Validate;

#[derive(Debug, Serialize, Deserialize, Validate, ToSchema)]
pub struct FilteredUser {
    pub id: String,
    #[validate(length(min = 3))]
    pub name: String,
    #[validate(email)]
    pub email: String,
}

impl From<User> for FilteredUser {
    fn from(value: User) -> Self {
        FilteredUser {
            id: value.id.to_string(),
            name: value.name,
            email: value.email,
        }
    }
}
