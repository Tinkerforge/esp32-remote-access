use diesel::{associations::{Associations, Identifiable}, deserialize::Queryable, prelude::Insertable, Selectable};
use ipnetwork::IpNetwork;
use crate::models::{users::User, chargers::Charger};

#[derive(Debug, Clone, Queryable, Selectable, Insertable, Identifiable, Associations)]
#[diesel(table_name = crate::schema::wg_keys)]
#[diesel(belongs_to(User))]
#[diesel(belongs_to(Charger))]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct WgKey {
    pub id: uuid::Uuid,
    pub user_id: uuid::Uuid,
    pub charger_id: i32,
    pub salt: String,
    pub in_use: bool,
    pub charger_pub: String,
    pub web_private: String,
    pub web_address: IpNetwork,
    pub charger_address: IpNetwork,
    pub connection_no: i32,
}
