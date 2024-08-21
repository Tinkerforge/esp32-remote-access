use diesel::{associations::Identifiable, deserialize::Queryable, prelude::Insertable, query_builder::AsChangeset, Selectable};
use ipnetwork::IpNetwork;

#[derive(Debug, Clone, Queryable, Selectable, Insertable, Identifiable, AsChangeset)]
#[diesel(table_name = crate::schema::chargers)]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct Charger {
    pub id: i32,
    pub password: String,
    pub name: String,
    pub management_private: String,
    pub charger_pub: String,
    pub wg_charger_ip: IpNetwork,
    pub psk: String,
    pub wg_server_ip: IpNetwork,
    pub webinterface_port: i32,
    pub firmware_version: String
}
