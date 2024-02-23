use diesel::{deserialize::Queryable, prelude::Insertable, Selectable};

#[derive(Debug, Clone, Queryable, Selectable, Insertable)]
#[diesel(table_name = crate::schema::chargers)]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct Charger {
    pub id: String,
    pub name: String,
    pub last_ip: Option<String>,
}
