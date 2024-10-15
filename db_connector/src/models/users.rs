use diesel::{associations::Identifiable, deserialize::Queryable, prelude::Insertable, Selectable};

#[derive(Debug, Clone, Queryable, Selectable, Insertable, Identifiable, PartialEq)]
#[diesel(table_name = crate::schema::users)]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct User {
    pub id: uuid::Uuid,
    pub name: String,
    pub email: String,
    pub login_key: String,
    pub email_verified: bool,
    pub secret: Vec<u8>,
    pub secret_nonce: Vec<u8>,
    pub secret_salt: Vec<u8>,
    pub login_salt: Vec<u8>,
}
