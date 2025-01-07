use diesel::prelude::*;
use super::users::User;

#[derive(Debug, Clone, Queryable, Selectable, Insertable, Identifiable, Associations, PartialEq)]
#[diesel(belongs_to(User))]
#[diesel(table_name = crate::schema::authorization_tokens)]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct AuthorizationToken {
    pub id: uuid::Uuid,
    pub user_id: uuid::Uuid,
    pub token: String,
    pub use_once: bool,
}
