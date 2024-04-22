use diesel::prelude::*;
use crate::models::users::User;

#[derive(Debug, Clone, Queryable, Selectable, Insertable, Identifiable, Associations)]
#[diesel(table_name = crate::schema::sessions)]
#[diesel(belongs_to(User))]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct Session {
    id: uuid::Uuid,
    user_id: uuid::Uuid,
}
