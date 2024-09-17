use std::{
    collections::{HashMap, HashSet},
    net::SocketAddr,
};

use actix_web::{get, web, HttpResponse, Responder};
use db_connector::models::wg_keys::WgKey;
use ipnetwork::IpNetwork;
use serde::Serialize;

use crate::{
    udp_server::{management::RemoteConnMeta, packet::ManagementResponse},
    BridgeState, DiscoveryCharger,
};

#[derive(Serialize, Debug)]
struct ServerState {
    clients: Vec<SocketAddr>,
    undiscovered_clients: Vec<RemoteConnMeta>,
    charger_management_map: Vec<SocketAddr>,
    charger_management_map_with_id: Vec<i32>,
    port_discovery: Vec<ManagementResponse>,
    charger_remote_conn_map: Vec<RemoteConnMeta>,
    undiscovered_chargers: HashMap<IpNetwork, HashSet<DiscoveryCharger>>,
    lost_connections: Vec<(i32, Vec<WgKey>)>,
}

#[get("/state")]
pub async fn state(brige_state: web::Data<BridgeState>) -> actix_web::Result<impl Responder> {
    let clients: Vec<SocketAddr> = {
        let web_client_map = brige_state.web_client_map.lock().unwrap();
        web_client_map
            .iter()
            .map(|(client, _)| client.to_owned())
            .collect()
    };

    let undiscovered_clients: Vec<RemoteConnMeta> = {
        let undiscoverd_clients = brige_state.undiscovered_clients.lock().unwrap();
        undiscoverd_clients
            .iter()
            .map(|(conn, _)| conn.clone())
            .collect()
    };

    let charger_management_map: Vec<SocketAddr> = {
        let charger_management_map = brige_state.charger_management_map.lock().unwrap();
        charger_management_map
            .iter()
            .map(|(sock, _)| sock.to_owned())
            .collect()
    };

    let charger_management_map_with_id: Vec<i32> = {
        let charger_management_map_with_id =
            brige_state.charger_management_map_with_id.lock().unwrap();
        charger_management_map_with_id
            .iter()
            .map(|(id, _)| id.to_owned())
            .collect()
    };

    let port_discovery: Vec<ManagementResponse> = {
        let port_discovery = brige_state.port_discovery.lock().unwrap();
        port_discovery
            .iter()
            .map(|(resp, _)| resp.clone())
            .collect()
    };

    let charger_remote_conn_map: Vec<RemoteConnMeta> = {
        let charger_remote_conn_map = brige_state.charger_remote_conn_map.lock().unwrap();
        charger_remote_conn_map
            .iter()
            .map(|(meta, _)| meta.clone())
            .collect()
    };

    let undiscovered_chargers = {
        let map = brige_state.undiscovered_chargers.lock().unwrap();
        map.clone()
    };

    let lost_connections: Vec<(i32, Vec<WgKey>)> = {
        let map = brige_state.lost_connections.lock().unwrap();
        map.iter().map(|(id, key)| (id.to_owned(), key.clone())).collect()
    };

    let state = ServerState {
        clients,
        undiscovered_clients,
        charger_management_map,
        port_discovery,
        charger_management_map_with_id,
        charger_remote_conn_map,
        undiscovered_chargers,
        lost_connections,
    };

    Ok(HttpResponse::Ok().json(state))
}
