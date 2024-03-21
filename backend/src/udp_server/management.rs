use std::net::SocketAddr;

use actix_web::web;

use crate::BridgeState;



#[derive(PartialEq, Hash, Eq, Debug)]
pub struct RemoteConnMeta {
    pub charger_id: i32,
    pub conn_no: i32,
}

#[repr(C)]
#[derive(Debug, Clone)]
pub enum ManagementCommandId {
    Connect,
    Disconnect,
    Ack,
}

#[repr(C)]
#[derive(Debug, Clone)]
pub struct ManagementCommand {
    pub command_id: ManagementCommandId,
    pub connection_no: i32,
    pub connection_uuid: u128,
}

#[derive(PartialEq, Eq, Hash, Debug)]
#[repr(C)]
pub struct ManagementResponse {
    pub charger_id: i32,
    pub connection_no: i32,
    pub connection_uuid: u128
}

pub fn try_port_discovery(state: &web::Data<BridgeState>, data: &[u8], addr: SocketAddr) -> bool {
    log::debug!("In port discovery: ");
    if data.len() != ::core::mem::size_of::<ManagementResponse>() {
        log::debug!("invalid size");
        return false
    }

    let response: ManagementResponse = unsafe {
        // using std::mem::transmute is more unsafe than std::ptr::read. https://users.rust-lang.org/t/isnt-a-pointer-cast-just-a-more-dangerous-transmute/47007
        std::ptr::read(data.as_ptr() as *const _)
    };

    {
        let mut set = state.port_discovery.lock().unwrap();
        log::debug!("{:?}", *set);
        log::debug!("{:?}", response);
        if !set.remove(&response) {
            return false
        }
    }

    let meta = RemoteConnMeta {
        charger_id: response.charger_id,
        conn_no: response.connection_no
    };

    {
        let mut map = state.undiscovered_clients.lock().unwrap();
        if let Some(r) = map.remove(&meta) {
            let mut map = state.web_client_map.lock().unwrap();
            map.insert(addr, r);
        }
    }

    let mut map = state.charger_remote_conn_map.lock().unwrap();
    map.insert(meta, addr);
    log::debug!("{:?}", *map);

    true
}
