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

use std::sync::Arc;

use argon2::{
    password_hash::{PasswordHashString, SaltString},
    Argon2, PasswordHasher, PasswordVerifier,
};

struct HashRequest {
    password: Vec<u8>,
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

impl Default for HasherManager {
    fn default() -> Self {
        let workers = (num_cpus::get_physical() / 2).max(1);
        Self::with_slots(Arc::new(tokio::sync::Semaphore::new(workers)))
    }
}

impl HasherManager {
    fn with_slots(slots: Arc<tokio::sync::Semaphore>) -> Self {
        let (tx, mut rx) = tokio::sync::mpsc::channel(10);

        actix::spawn(async move {
            let hasher = Arc::new(Argon2::default());

            while let Some(request) = rx.recv().await {
                // Wait before dispatching so the bounded input channel cannot
                // be drained into an unbounded thread-pool queue.
                let permit = slots.clone().acquire_owned().await.unwrap();
                let hasher = hasher.clone();
                tokio::task::spawn_blocking(move || {
                    let _permit = permit;
                    match request {
                        Request::Hash(hash_request) => {
                            if hash_request.responder.is_closed() {
                                return;
                            }
                            let result = match hasher
                                .hash_password(&hash_request.password, &hash_request.salt)
                            {
                                Ok(hash) => {
                                    let string = hash.serialize();
                                    Ok(string)
                                }
                                Err(e) => Err(e),
                            };
                            let _ = hash_request.responder.send(result);
                        }
                        Request::Verify(verify_request) => {
                            if verify_request.responder.is_closed() {
                                return;
                            }
                            let hash = verify_request.hash.password_hash();
                            let result = hasher.verify_password(&verify_request.password, &hash);
                            let _ = verify_request.responder.send(result);
                        }
                    }
                });
            }
        });

        Self { tx }
    }
}

impl HasherManager {
    pub async fn hash_password(
        &self,
        password: Vec<u8>,
        salt: SaltString,
    ) -> argon2::password_hash::Result<PasswordHashString> {
        let (responder_tx, responder_rx) = tokio::sync::oneshot::channel();
        let request = Request::Hash(HashRequest {
            password,
            salt,
            responder: responder_tx,
        });
        self.tx
            .send(request)
            .await
            .map_err(|_| argon2::password_hash::Error::Crypto)?;
        responder_rx
            .await
            .map_err(|_| argon2::password_hash::Error::Crypto)?
    }

    pub async fn verify_password(
        &self,
        hash: PasswordHashString,
        password: Vec<u8>,
    ) -> argon2::password_hash::Result<()> {
        let (responder_tx, responder_rx) = tokio::sync::oneshot::channel();
        let request = Request::Verify(VerifyRequest {
            hash,
            password,
            responder: responder_tx,
        });
        self.tx
            .send(request)
            .await
            .map_err(|_| argon2::password_hash::Error::Crypto)?;
        responder_rx
            .await
            .map_err(|_| argon2::password_hash::Error::Crypto)?
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cancelled_request() -> Request {
        let (responder, _receiver) = tokio::sync::oneshot::channel();
        Request::Hash(HashRequest {
            password: b"password".to_vec(),
            salt: SaltString::encode_b64(b"test salt").unwrap(),
            responder,
        })
    }

    #[actix_web::test]
    async fn dispatcher_applies_backpressure_and_recovers_after_cancellation() {
        let slots = Arc::new(tokio::sync::Semaphore::new(1));
        let occupied = slots.clone().acquire_owned().await.unwrap();
        let hasher = HasherManager::with_slots(slots);
        hasher.tx.send(cancelled_request()).await.unwrap();
        tokio::time::timeout(std::time::Duration::from_secs(1), async {
            while hasher.tx.capacity() != 10 {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        // One request is waiting for the occupied worker; exactly ten more
        // can queue, regardless of how often the dispatcher is scheduled.
        for _ in 0..10 {
            assert!(hasher.tx.try_send(cancelled_request()).is_ok());
        }
        tokio::task::yield_now().await;
        assert!(matches!(
            hasher.tx.try_send(cancelled_request()),
            Err(tokio::sync::mpsc::error::TrySendError::Full(_))
        ));
        drop(occupied);
        let hash = tokio::time::timeout(
            std::time::Duration::from_secs(10),
            hasher.hash_password(
                b"password".to_vec(),
                SaltString::encode_b64(b"test salt").unwrap(),
            ),
        )
        .await
        .unwrap()
        .unwrap();
        hasher
            .verify_password(hash.clone(), b"password".to_vec())
            .await
            .unwrap();
        assert!(hasher
            .verify_password(hash, b"wrong".to_vec())
            .await
            .is_err());
    }
}
