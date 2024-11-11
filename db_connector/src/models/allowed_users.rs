use diesel::prelude::*;
use super::{users::User, chargers::Charger};

#[derive(Debug, Clone, Queryable, Selectable, Insertable, Identifiable, Associations, PartialEq)]
#[diesel(belongs_to(User))]
#[diesel(belongs_to(Charger))]
#[diesel(table_name = crate::schema::allowed_users)]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct AllowedUser {
    pub id: uuid::Uuid,
    pub user_id: uuid::Uuid,
    pub charger_id: uuid::Uuid,
    pub charger_uid: i32,
    pub valid: bool,
    pub name: Option<Vec<u8>>,
    pub note: Option<Vec<u8>>,
}
