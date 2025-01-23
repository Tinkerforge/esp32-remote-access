use diesel::{deserialize::Queryable, prelude::Insertable, Selectable};

#[derive(Debug, Clone, Queryable, Selectable, Insertable)]
#[diesel(table_name = crate::schema::verification)]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct Verification {
    pub id: uuid::Uuid,
    pub user: uuid::Uuid,
    pub expiration: chrono::NaiveDateTime,
}

impl Verification {
    pub fn new(user: uuid::Uuid) -> Self {
        Self {
            id: uuid::Uuid::new_v4(),
            user,
            expiration: chrono::Utc::now().naive_utc() + chrono::Duration::days(1),
        }
    }
}
