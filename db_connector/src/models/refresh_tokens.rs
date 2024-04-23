use diesel::prelude::*;
use crate::models::users::User;

#[derive(Debug, Clone, Queryable, Selectable, Insertable, Identifiable, Associations)]
#[diesel(table_name = crate::schema::refresh_tokens)]
#[diesel(belongs_to(User))]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct RefreshToken {
    pub id: uuid::Uuid,
    pub user_id: uuid::Uuid,
    pub expiration: i64,
}
