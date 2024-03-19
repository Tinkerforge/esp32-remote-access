use diesel::{associations::Identifiable, deserialize::Queryable, prelude::Insertable, Selectable};
use ipnetwork::IpNetwork;

#[derive(Debug, Clone, Queryable, Selectable, Insertable, Identifiable)]
#[diesel(table_name = crate::schema::chargers)]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct Charger {
    pub id: i32,
    pub name: String,
    pub last_ip: Option<IpNetwork>,
    pub management_private: String,
    pub charger_pub: String,
    pub wg_charger_ip: IpNetwork,
    pub wg_server_ip: IpNetwork,
}
