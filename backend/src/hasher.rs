/* esp32-remote-access
 * Copyright (C) 2025 Frederic Henrichs <frederic@tinkerforge.com>
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

use argon2::{Argon2, PasswordHasher, PasswordVerifier, password_hash::{PasswordHashString, SaltString}};

struct HashRequest {
    password: String,
    salt: SaltString,
    responder: tokio::sync::oneshot::Sender<argon2::password_hash::Result<PasswordHashString>>,
}

struct VerifyRequest {
    hash: PasswordHashString,
    password: Vec<u8>,
    responder: tokio::sync::oneshot::Sender<argon2::password_hash::Result<()>>,
}

enum Request {
    Hash(HashRequest),
    Verify(VerifyRequest),
}

pub struct HasherManager {
    tx: tokio::sync::mpsc::Sender<Request>,
}

impl HasherManager {
    pub fn new() -> Self {
        let (tx, mut rx) = tokio::sync::mpsc::channel(10);

        actix::spawn(async move {
            let hasher = Argon2::default();
            while let Some(request) = rx.recv().await {
                match request {
                    Request::Hash(hash_request) => {
                        let result = match hasher.hash_password(
                            hash_request.password.as_bytes(),
                            &hash_request.salt,
                        ) {
                            Ok(hash) => Ok(hash.serialize()),
                            Err(e) => Err(e),
                        };
                        let _ = hash_request.responder.send(result);
                    },
                    Request::Verify(verify_request) => {
                        let hash = verify_request.hash.password_hash();
                        let result = hasher.verify_password(
                            &verify_request.password,
                            &hash,
                        );
                        let _ = verify_request.responder.send(result);
                    }
                }
            }
        });

        Self {
            tx
        }
    }

    pub async fn hash_password(&self, password: String, salt: SaltString) -> argon2::password_hash::Result<PasswordHashString> {
        let (responder_tx, responder_rx) = tokio::sync::oneshot::channel();
        let request = Request::Hash(HashRequest {
            password,
            salt,
            responder: responder_tx,
        });
        let _ = self.tx.send(request).await;
        responder_rx.await.unwrap()
    }

    pub async fn verify_password(&self, hash: PasswordHashString, password: Vec<u8>) -> argon2::password_hash::Result<()> {
        let (responder_tx, responder_rx) = tokio::sync::oneshot::channel();
        let request = Request::Verify(VerifyRequest {
            hash,
            password,
            responder: responder_tx,
        });
        let _ = self.tx.send(request).await;
        responder_rx.await.unwrap()
    }
}
