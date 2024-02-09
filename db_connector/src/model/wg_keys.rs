use diesel::{deserialize::Queryable, Selectable, prelude::Insertable};

#[derive(Debug, Clone, Queryable, Selectable, Insertable)]
#[diesel(table_name = crate::schema::wg_keys)]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct WgKey {
    pub id: uuid::Uuid,
    pub charger: String,
    pub in_use: bool,
    pub charger_pub: String,
    pub user_private: String
}
