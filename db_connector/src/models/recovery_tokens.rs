use diesel::prelude::*;
use uuid::Uuid;

#[derive(Debug, Clone, Queryable, Selectable, Insertable, Identifiable)]
#[diesel(table_name = crate::schema::recovery_tokens)]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct RecoveryToken {
    pub id: Uuid,
    pub user_id: Uuid,
    pub created: i64,
}
