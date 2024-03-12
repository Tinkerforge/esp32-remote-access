use actix::prelude::*;
use actix::{Actor, StreamHandler};
use actix_web::web::Bytes;
use actix_web::{get, web, HttpRequest, HttpResponse};
use actix_web_actors::ws;
use actix_web_validator::Query;
use db_connector::models::{chargers::Charger, wg_keys::WgKey};
use diesel::prelude::*;
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::str::FromStr;
use validator::{Validate, ValidationError};

use crate::{
    error::Error,
    utils::{get_connection, web_block_unpacked},
    AppState, BridgeState,
};

#[derive(Deserialize, Serialize, Validate)]
struct WsQuery {
    #[validate(custom = "validate_key_id")]
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
    pub charger_id: String,
    pub peer_sock_addr: SocketAddr,
    pub app_state: web::Data<AppState>,
    pub bridge_state: web::Data<BridgeState>,
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
                match self.bridge_state.socket.send_to(&msg, self.peer_sock_addr) {
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
            _ => (),
        }
    }

    fn started(&mut self, ctx: &mut Self::Context) {
        let mut client_map = self.bridge_state.web_client_map.lock().unwrap();
        client_map.insert(self.peer_sock_addr, ctx.address().recipient::<Message>());
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

        if let Ok(mut client_map) = self.bridge_state.web_client_map.lock() {
            client_map.remove(&self.peer_sock_addr);
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

#[get("/ws")]
async fn start_ws(
    req: HttpRequest,
    stream: web::Payload,
    state: web::Data<AppState>,
    uid: crate::models::uuid::Uuid,
    key_id: Query<WsQuery>,
    bridge_state: web::Data<BridgeState>,
) -> Result<HttpResponse, actix_web::Error> {
    use db_connector::schema::chargers::dsl as chargers;
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

        if let Err(_err) = diesel::update(wg_keys::wg_keys)
            .filter(wg_keys::id.eq(&key_id))
            .set(wg_keys::in_use.eq(true))
            .execute(&mut conn)
        {
            return Err(Error::InternalError);
        }

        Ok(keys)
    })
    .await?;

    if !keys.user_id.eq(&uid.into()) {
        return Err(Error::UserIsNotOwner.into());
    }

    let mut conn = get_connection(&state)?;
    let charger = keys.charger_id.clone();
    let peer_address = web_block_unpacked(move || {
        let charger: Charger = match chargers::chargers
            .find(charger)
            .select(Charger::as_select())
            .get_result(&mut conn)
        {
            Ok(c) => c,
            Err(_err) => return Err(Error::InternalError),
        };

        match charger.last_ip {
            Some(ip) => Ok(ip),
            None => Err(Error::ChargerNotSeenYet),
        }
    })
    .await?;

    let peer_address = peer_address.to_string();
    let sock_addr = format!(
        "{}:{}",
        peer_address[0..(peer_address.len() - 3)].to_string(),
        keys.wg_port as u16
    );
    let peer_sock_addr: SocketAddr = match sock_addr.parse() {
        Ok(sock_addr) => sock_addr,
        Err(err) => {
            log::error!("Error while parsing socket_addr: {}", err);
            return Err(Error::InternalError.into());
        }
    };

    let client = WebClient {
        key_id: keys.id,
        charger_id: keys.charger_id,
        app_state: state.clone(),
        peer_sock_addr,
        bridge_state,
    };

    let resp = ws::start(client, &req, stream);

    if let Err(err) = &resp {
        log::debug!("{:?}", err.to_string());
    }

    resp
}

#[cfg(test)]
mod tests {
    // #[actix_web::test]
    // async fn test_connecting_ws() {

    // }
}
