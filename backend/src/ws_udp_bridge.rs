/* esp32-remote-access
 * Copyright (C) 2024 Frederic Henrichs <frederic@tinkerforge.com>
 *
 * This library is free software; you can redistribute it and/or
 * modify it under the terms of the GNU Lesser General Public
 * License as published by the Free Software Foundation; either
 * version 2 of the License, or (at your option) any later version.
 *
 * This library is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU
 * Lesser General Public License for more details.
 *
 * You should have received a copy of the GNU Lesser General Public
 * License along with this library; if not, write to the
 * Free Software Foundation, Inc., 59 Temple Place - Suite 330,
 * Boston, MA 02111-1307, USA.
 */

use actix::prelude::*;
use actix::{Actor, StreamHandler};
use actix_web::web::Bytes;
use actix_web::{get, web, HttpRequest, HttpResponse};
use actix_web_actors::ws;
use actix_web_validator::Query;
use db_connector::models::wg_keys::WgKey;
use diesel::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::str::FromStr;
use std::sync::{Arc, Mutex};
use std::time::Instant;
use validator::{Validate, ValidationError};

use crate::udp_server::management::RemoteConnMeta;
use crate::udp_server::packet::{
    ManagementCommand, ManagementCommandId, ManagementCommandPacket, ManagementPacket,
    ManagementPacketHeader, ManagementResponseV2,
};
use crate::udp_server::socket::ManagementSocket;
use crate::{
    error::Error,
    utils::{get_connection, web_block_unpacked},
    AppState, BridgeState,
};

#[derive(Deserialize, Serialize, Validate)]
struct WsQuery {
    #[validate(custom(function = validate_key_id))]
    pub key_id: String,
}

fn validate_key_id(key_id: &str) -> Result<(), ValidationError> {
    match uuid::Uuid::from_str(key_id) {
        Ok(_) => Ok(()),
        Err(_err) => Err(ValidationError::new("key_id is not a valid Uuid")),
    }
}

pub struct WebClient {
    pub key_id: uuid::Uuid,
    pub charger_id: uuid::Uuid,
    pub app_state: web::Data<AppState>,
    pub bridge_state: web::Data<BridgeState>,
    pub conn_no: i32,
}

#[derive(Message)]
#[rtype(result = "()")]
pub struct Message(pub Bytes);

impl Actor for WebClient {
    type Context = ws::WebsocketContext<Self>;
}

impl Handler<Message> for WebClient {
    type Result = ();

    fn handle(&mut self, msg: Message, ctx: &mut Self::Context) -> Self::Result {
        ctx.binary(msg.0)
    }
}

impl StreamHandler<Result<ws::Message, ws::ProtocolError>> for WebClient {
    fn handle(&mut self, item: Result<ws::Message, ws::ProtocolError>, ctx: &mut Self::Context) {
        match item {
            Ok(ws::Message::Ping(msg)) => ctx.pong(&msg),
            Ok(ws::Message::Binary(msg)) => {
                let peer_sock_addr = {
                    let meta = RemoteConnMeta {
                        charger_id: self.charger_id.clone(),
                        conn_no: self.conn_no,
                    };
                    let map = self.bridge_state.charger_remote_conn_map.lock().unwrap();
                    match map.get(&meta) {
                        Some(addr) => {
                            let addr = addr.to_owned();
                            addr
                        }
                        None => {
                            return;
                        }
                    }
                };

                match self.bridge_state.socket.send_to(&msg, peer_sock_addr) {
                    Ok(s) => {
                        if s < msg.len() {
                            log::error!("Sent incomplete message to charger '{}'", self.charger_id);
                        }
                    }
                    Err(_err) => {
                        log::error!(
                            "Failed to send message to charger '{}': {}",
                            self.charger_id,
                            _err
                        );
                    }
                }
            }
            Ok(ws::Message::Close(_)) => {
                ctx.close(None);
                self.finished(ctx);
            }
            Err(err) => {
                log::error!("Websocket error: {}", err.to_string());
            }
            _ => (),
        }
    }

    fn started(&mut self, ctx: &mut Self::Context) {
        let meta = RemoteConnMeta {
            charger_id: self.charger_id.clone(),
            conn_no: self.conn_no,
        };

        let peer_sock_addr = {
            let map = self.bridge_state.charger_remote_conn_map.lock().unwrap();
            match map.get(&meta) {
                Some(addr) => addr.to_owned(),
                None => {
                    drop(map);
                    let mut map = self.bridge_state.undiscovered_clients.lock().unwrap();
                    map.insert(meta, ctx.address().recipient::<Message>());
                    return;
                }
            }
        };

        let mut client_map = self.bridge_state.web_client_map.lock().unwrap();
        client_map.insert(peer_sock_addr, ctx.address().recipient::<Message>());
    }

    fn finished(&mut self, ctx: &mut Self::Context) {
        use db_connector::schema::wg_keys::dsl::*;

        log::debug!("Closed connection to charger '{}'", self.charger_id);
        let mut conn = match self.app_state.pool.get() {
            Ok(conn) => conn,
            Err(_err) => {
                log::error!(
                    "Failed to release connection '{}' for charger '{}'",
                    self.key_id.to_string(),
                    self.charger_id
                );
                return;
            }
        };

        let meta = RemoteConnMeta {
            charger_id: self.charger_id.clone(),
            conn_no: self.conn_no,
        };
        {
            let mut map = self.bridge_state.charger_remote_conn_map.lock().unwrap();
            if let Some(addr) = map.get(&meta) {
                let mut map = self.bridge_state.web_client_map.lock().unwrap();
                map.remove(&addr);
            }

            map.remove(&meta);
        }

        {
            let mut lost_map = self.bridge_state.lost_connections.lock().unwrap();
            let _ = lost_map.remove(&self.charger_id);
        }

        {
            let command = ManagementCommand {
                command_id: ManagementCommandId::Disconnect,
                connection_no: self.conn_no,
                connection_uuid: uuid::Uuid::new_v4().as_u128(),
            };
            let header = ManagementPacketHeader {
                magic: 0x1234,
                length: std::mem::size_of::<ManagementCommand>() as u16,
                seq_number: 0,
                version: 1,
                p_type: 0x00,
            };

            let packet = ManagementCommandPacket { header, command };
            let map = self
                .bridge_state
                .charger_management_map_with_id
                .lock()
                .unwrap();
            if let Some(sock) = map.get(&self.charger_id) {
                let mut sock = sock.lock().unwrap();
                sock.send_packet(ManagementPacket::CommandPacket(packet));
            }
        }

        match diesel::update(wg_keys)
            .filter(id.eq(self.key_id))
            .set(in_use.eq(false))
            .execute(&mut conn)
        {
            Ok(_) => (),
            Err(_err) => {
                log::error!(
                    "Failed to release connection '{}' for charger '{}'",
                    self.key_id.to_string(),
                    self.charger_id
                );
            }
        }

        ctx.stop();
    }
}

pub fn open_connection(
    conn_no: i32,
    charger_id: uuid::Uuid,
    management_sock: Arc<Mutex<ManagementSocket>>,
    port_discovery: Arc<Mutex<HashMap<ManagementResponseV2, Instant>>>,
) -> Result<(), Error> {
    let conn_uuid = uuid::Uuid::new_v4();
    let command = ManagementCommand {
        command_id: ManagementCommandId::Connect,
        connection_no: conn_no,
        connection_uuid: conn_uuid.as_u128(),
    };
    let response = ManagementResponseV2 {
        charger_id: charger_id.as_u128(),
        connection_no: conn_no,
        connection_uuid: conn_uuid.as_u128(),
    };

    let header = ManagementPacketHeader {
        magic: 0x1234,
        length: std::mem::size_of::<ManagementCommand>() as u16,
        seq_number: 0,
        version: 1,
        p_type: 0x00,
    };

    let packet = ManagementCommandPacket { header, command };
    let mut sock = management_sock.lock().unwrap();
    sock.send_packet(ManagementPacket::CommandPacket(packet));
    let mut set = port_discovery.lock().unwrap();
    set.insert(response, Instant::now());

    Ok(())
}

#[get("/ws")]
async fn start_ws(
    req: HttpRequest,
    stream: web::Payload,
    state: web::Data<AppState>,
    uid: crate::models::uuid::Uuid,
    key_id: Query<WsQuery>,
    bridge_state: web::Data<BridgeState>,
) -> Result<HttpResponse, actix_web::Error> {
    use db_connector::schema::wg_keys::dsl as wg_keys;

    let mut conn = get_connection(&state)?;
    let keys: WgKey = web_block_unpacked(move || {
        let key_id = uuid::Uuid::from_str(&key_id.key_id).unwrap();

        let keys: WgKey = match wg_keys::wg_keys
            .filter(wg_keys::id.eq(&key_id))
            .select(WgKey::as_select())
            .get_result(&mut conn)
        {
            Ok(keys) => keys,
            Err(_err) => return Err(Error::WgKeysDoNotExist),
        };

        if keys.in_use {
            return Err(Error::WgKeyAlreadyInUse);
        }

        Ok(keys)
    })
    .await?;

    if !keys.user_id.eq(&uid.into()) {
        return Err(Error::Unauthorized.into());
    }

    let client = WebClient {
        key_id: keys.id,
        charger_id: keys.charger_id,
        app_state: state.clone(),
        conn_no: keys.connection_no,
        bridge_state: bridge_state.clone(),
    };

    let management_sock = {
        let map = bridge_state.charger_management_map_with_id.lock().unwrap();
        let management_sock = match map.get(&keys.charger_id) {
            Some(sock) => sock.clone(),
            None => return Err(Error::ChargerDisconnected.into()),
        };
        management_sock
    };

    open_connection(
        keys.connection_no,
        keys.charger_id,
        management_sock,
        bridge_state.port_discovery.clone(),
    )?;

    let resp = ws::start(client, &req, stream);

    if resp.is_ok() {
        let mut conn = get_connection(&state)?;
        use db_connector::schema::wg_keys::dsl::*;
        web_block_unpacked(move || {
            if let Err(_err) = diesel::update(wg_keys)
                .filter(id.eq(&keys.id))
                .set(in_use.eq(true))
                .execute(&mut conn)
            {
                return Err(Error::InternalError);
            }
            Ok(())
        }).await?;
    }

    resp
}

#[cfg(test)]
mod tests {
    // #[actix_web::test]
    // async fn test_connecting_ws() {

    // }
}
