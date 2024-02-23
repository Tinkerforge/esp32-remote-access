use diesel::{deserialize::Queryable, prelude::Insertable, Selectable};
use ipnetwork::IpNetwork;

#[derive(Debug, Clone, Queryable, Selectable, Insertable)]
#[diesel(table_name = crate::schema::wg_keys)]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct WgKey {
    pub id: uuid::Uuid,
    pub user_id: uuid::Uuid,
    pub charger: String,
    pub in_use: bool,
    pub charger_pub: String,
    pub user_private: String,
    pub web_address: IpNetwork,
    pub charger_address: IpNetwork,
}
