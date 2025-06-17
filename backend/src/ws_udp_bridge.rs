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

use actix::clock::interval;
use actix::prelude::*;
use actix_web::rt::pin;
use actix_web::web::Bytes;
use actix_web::{get, rt, web, HttpRequest, HttpResponse};
use actix_web_validator::Query;
use actix_ws::{AggregatedMessage, Session};
use db_connector::models::wg_keys::WgKey;
use diesel::prelude::*;
use futures_util::future::Either;
use futures_util::lock::Mutex;
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::str::FromStr;
use std::sync::Arc;
use std::time::{Duration, Instant};
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

const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(5);
const CLIENT_TIMEOUT: Duration = Duration::from_secs(15);

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
    key_id: uuid::Uuid,
    charger_id: uuid::Uuid,
    app_state: web::Data<AppState>,
    bridge_state: web::Data<BridgeState>,
    conn_no: i32,
    session: Session,
}

impl WebClient {
    pub async fn new(
        key_id: uuid::Uuid,
        charger_id: uuid::Uuid,
        app_state: web::Data<AppState>,
        bridge_state: web::Data<BridgeState>,
        conn_no: i32,
        session: Session,
    ) -> Self {
        let meta = RemoteConnMeta {
            charger_id,
            conn_no,
        };

        {
            let map = bridge_state.charger_remote_conn_map.lock().await;
            match map.get(&meta) {
                Some(addr) => {
                    let mut client_map = bridge_state.web_client_map.lock().await;
                    client_map.insert(*addr, session.clone());
                }
                None => {
                    drop(map);
                    let mut map = bridge_state.undiscovered_clients.lock().await;
                    map.insert(meta, session.clone());
                }
            }
        }

        Self {
            key_id,
            charger_id,
            app_state,
            bridge_state,
            conn_no,
            session,
        }
    }

    pub async fn handle_message(&mut self, msg: AggregatedMessage, last_heartbeat: &mut Instant) {
        match msg {
            AggregatedMessage::Ping(msg) => {
                self.session.pong(&msg).await.unwrap();
                *last_heartbeat = Instant::now();
            }
            AggregatedMessage::Pong(_) => {
                *last_heartbeat = Instant::now();
            }
            AggregatedMessage::Binary(msg) => {
                let peer_sock_addr = {
                    let meta = RemoteConnMeta {
                        charger_id: self.charger_id,
                        conn_no: self.conn_no,
                    };
                    let map = self.bridge_state.charger_remote_conn_map.lock().await;
                    match map.get(&meta) {
                        Some(addr) => {
                            addr.to_owned()
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
            msg => log::info!("/ws got other msg: {:?}", msg),
        }
    }

    pub async fn stop(self) {
        use db_connector::schema::wg_keys::dsl::*;

        log::debug!("Closed connection to charger '{}'", self.charger_id);
        let mut conn = match self.app_state.pool.get() {
            Ok(conn) => conn,
            Err(_err) => {
                log::error!(
                    "Failed to release connection '{}' for charger '{}'",
                    self.key_id,
                    self.charger_id
                );
                return;
            }
        };

        let meta = RemoteConnMeta {
            charger_id: self.charger_id,
            conn_no: self.conn_no,
        };
        {
            let mut map = self.bridge_state.charger_remote_conn_map.lock().await;
            if let Some(addr) = map.get(&meta) {
                let mut map = self.bridge_state.web_client_map.lock().await;
                map.remove(addr);
            }

            map.remove(&meta);
        }

        {
            let mut lost_map = self.bridge_state.lost_connections.lock().await;
            let _ = lost_map.remove(&self.charger_id);
        }
        {
            let mut map = self.bridge_state.undiscovered_clients.lock().await;
            let _ = map.remove(&meta);
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
                .await;
            if let Some(sock) = map.get(&self.charger_id) {
                let mut sock = sock.lock().await;
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
                    self.key_id,
                    self.charger_id
                );
            }
        }

        self.session.close(None).await.ok();
    }
}

#[derive(Message)]
#[rtype(result = "()")]
pub struct Message(pub Bytes);

pub async fn open_connection(
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
    let mut sock = management_sock.lock().await;
    sock.send_packet(ManagementPacket::CommandPacket(packet));
    let mut set = port_discovery.lock().await;
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

    let user_id: uuid::Uuid = uid.into();
    if !keys.user_id.eq(&user_id) {
        return Err(Error::Unauthorized.into());
    }

    let management_sock = {
        let map = bridge_state.charger_management_map_with_id.lock().await;
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
    )
    .await?;

    let (resp, mut session, stream) = actix_ws::handle(&req, stream)?;
    let mut stream = stream
        .aggregate_continuations()
        .max_continuation_size(2_usize.pow(20));

    if resp.status() == 101 {
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
        })
        .await?;
    }

    rt::spawn(async move {
        let mut client = WebClient::new(
            keys.id,
            keys.charger_id,
            state,
            bridge_state,
            keys.connection_no,
            session.clone(),
        )
        .await;

        let mut last_heartbeat = Instant::now();
        let mut interval = interval(HEARTBEAT_INTERVAL);
        loop {
            let tick = interval.tick();
            pin!(tick);

            match futures_util::future::select(stream.next(), tick).await {
                Either::Left((Some(Ok(AggregatedMessage::Close(_))), _)) => break,
                Either::Left((Some(Ok(msg)), _)) => {
                    client.handle_message(msg, &mut last_heartbeat).await
                }
                Either::Left((Some(err), _)) => {
                    log::error!("Websocket Error during connection: {:?}", err);
                    break;
                }
                Either::Left((None, _)) => break,
                Either::Right(_) => {
                    if Instant::now().duration_since(last_heartbeat) > CLIENT_TIMEOUT {
                        log::debug!("Client quietly quit.");
                        break;
                    }
                    let _ = session.ping(b"").await;
                }
            }
        }
        client.stop().await;
    });

    Ok(resp)
}

#[cfg(test)]
mod tests {
    // #[actix_web::test]
    // async fn test_connecting_ws() {

    // }
}
