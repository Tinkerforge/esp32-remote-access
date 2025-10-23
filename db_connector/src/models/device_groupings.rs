use super::users::User;
use diesel::prelude::*;

#[derive(
    Debug, Clone, Queryable, Selectable, Insertable, Identifiable, Associations, PartialEq,
)]
#[diesel(belongs_to(User))]
#[diesel(table_name = crate::schema::device_groupings)]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct DeviceGrouping {
    pub id: uuid::Uuid,
    pub name: String,
    pub user_id: uuid::Uuid,
}
