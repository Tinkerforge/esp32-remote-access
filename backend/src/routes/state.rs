use std::{
    collections::{HashMap, HashSet},
    net::SocketAddr,
};

use actix_web::{get, web, HttpResponse, Responder};
use ipnetwork::IpNetwork;
use serde::Serialize;

use crate::{
    udp_server::{management::RemoteConnMeta, packet::ManagementResponseV2},
    BridgeState, DiscoveryCharger,
};

#[derive(Serialize, Debug, PartialEq, Clone)]
pub struct ServerState {
    pub clients: Vec<SocketAddr>,
    pub undiscovered_clients: Vec<RemoteConnMeta>,
    pub charger_management_map: Vec<SocketAddr>,
    pub charger_management_map_with_id: Vec<String>,
    pub port_discovery: Vec<ManagementResponseV2>,
    pub charger_remote_conn_map: Vec<RemoteConnMeta>,
    pub undiscovered_chargers: HashMap<IpNetwork, HashSet<DiscoveryCharger>>,
    pub lost_connections: Vec<(String, Vec<i32>)>,
}

#[get("/state")]
pub async fn state(brige_state: web::Data<BridgeState>) -> actix_web::Result<impl Responder> {
    let clients: Vec<SocketAddr> = {
        let web_client_map = brige_state.web_client_map.lock().await;
        web_client_map
            .iter()
            .map(|(client, _)| client.to_owned())
            .collect()
    };

    let undiscovered_clients: Vec<RemoteConnMeta> = {
        let undiscoverd_clients = brige_state.undiscovered_clients.lock().await;
        undiscoverd_clients
            .iter()
            .map(|(conn, _)| conn.clone())
            .collect()
    };

    let charger_management_map: Vec<SocketAddr> = {
        let charger_management_map = brige_state.charger_management_map.lock().await;
        charger_management_map
            .iter()
            .map(|(sock, _)| sock.to_owned())
            .collect()
    };

    let charger_management_map_with_id: Vec<String> = {
        let charger_management_map_with_id =
            brige_state.charger_management_map_with_id.lock().await;
        charger_management_map_with_id
            .iter()
            .map(|(id, _)| id.to_string())
            .collect()
    };

    let port_discovery: Vec<ManagementResponseV2> = {
        let port_discovery = brige_state.port_discovery.lock().await;
        port_discovery
            .iter()
            .map(|(resp, _)| resp.clone())
            .collect()
    };

    let charger_remote_conn_map: Vec<RemoteConnMeta> = {
        let charger_remote_conn_map = brige_state.charger_remote_conn_map.lock().await;
        charger_remote_conn_map
            .iter()
            .map(|(meta, _)| meta.clone())
            .collect()
    };

    let undiscovered_chargers = {
        let map = brige_state.undiscovered_chargers.lock().await;
        map.clone()
    };

    let lost_connections: Vec<(String, Vec<i32>)> = {
        let map = brige_state.lost_connections.lock().await;
        map.iter()
            .map(|(id, conns)| {
                (
                    id.to_string(),
                    conns.into_iter().map(|(conn_no, _)| *conn_no).collect(),
                )
            })
            .collect()
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
