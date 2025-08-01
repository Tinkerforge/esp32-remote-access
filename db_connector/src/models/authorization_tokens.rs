use super::users::User;
use diesel::prelude::*;

#[derive(
    Debug, Clone, Queryable, Selectable, Insertable, Identifiable, Associations, PartialEq,
)]
#[diesel(belongs_to(User))]
#[diesel(table_name = crate::schema::authorization_tokens)]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct AuthorizationToken {
    pub id: uuid::Uuid,
    pub user_id: uuid::Uuid,
    pub token: String,
    pub use_once: bool,
    pub name: String,
    pub created_at: chrono::NaiveDateTime,
    pub last_used_at: Option<chrono::NaiveDateTime>,
}
