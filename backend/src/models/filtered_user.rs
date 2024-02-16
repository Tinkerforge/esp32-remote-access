use db_connector::models::users::User;
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct FilteredUser {
    pub id: String,
    pub name: String,
    pub email: String
}

impl From<User> for FilteredUser {
    fn from(value: User) -> Self {
        FilteredUser {
            id: value.id.to_string(),
            name: value.name,
            email: value.email
        }
    }
}
