use diesel::{deserialize::Queryable, Selectable, prelude::Insertable};

#[derive(Debug, Clone, Queryable, Selectable, Insertable)]
#[diesel(table_name = crate::schema::allowed_users)]
#[diesel(check_for_backend(diesel::pg::Pg))]
#[diesel(belongs_to(User))]
pub struct AllowedUser {
    pub id: uuid::Uuid,
    pub user: uuid::Uuid,
    pub charger: String,
    pub is_owner: bool,
}
