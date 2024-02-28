use diesel::{associations::Identifiable, deserialize::Queryable, prelude::Insertable, Selectable};

#[derive(Debug, Clone, Queryable, Selectable, Insertable, Identifiable)]
#[diesel(table_name = crate::schema::chargers)]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct Charger {
    pub id: String,
    pub name: String,
    pub last_ip: Option<String>,
}
