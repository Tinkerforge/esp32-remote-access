use super::chargers::Charger;
use super::device_groupings::DeviceGrouping;
use diesel::prelude::*;

#[derive(
    Debug, Clone, Queryable, Selectable, Insertable, Identifiable, Associations, PartialEq,
)]
#[diesel(belongs_to(DeviceGrouping, foreign_key = grouping_id))]
#[diesel(belongs_to(Charger, foreign_key = charger_id))]
#[diesel(table_name = crate::schema::device_grouping_members)]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct DeviceGroupingMember {
    pub id: uuid::Uuid,
    pub grouping_id: uuid::Uuid,
    pub charger_id: uuid::Uuid,
    pub added_at: chrono::NaiveDateTime,
}
