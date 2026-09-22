use super::socket::ManagementSocket;
use futures_util::lock::Mutex;
use std::{collections::HashMap, net::SocketAddr, sync::Arc, time::Duration};

type Socket<'a> = Arc<Mutex<ManagementSocket<'a>>>;
type AddressMap<'a> = Mutex<HashMap<SocketAddr, Socket<'a>>>;
type IdMap<'a> = Mutex<HashMap<uuid::Uuid, Socket<'a>>>;

/// Writers always acquire address -> ID. Socket locks are only tried while
/// holding these maps, never awaited. Updating both indices has no yield point.
pub(super) async fn publish<'a>(
    by_addr: &AddressMap<'a>,
    by_id: &IdMap<'a>,
    id: uuid::Uuid,
    addr: SocketAddr,
    mut socket: ManagementSocket<'a>,
    generation: u64,
) -> Option<Socket<'a>> {
    socket.discovery_generation = generation;
    let socket = Arc::new(Mutex::new(socket));
    let mut by_addr = by_addr.lock().await;
    let mut by_id = by_id.lock().await;
    if by_addr.contains_key(&addr) {
        return None;
    }
    if let Some(current) = by_id.get(&id) {
        let current_guard = current.try_lock()?;
        if current_guard.discovery_generation >= generation {
            return None;
        }
        let old_addr = current_guard.get_remote_address();
        if by_addr
            .get(&old_addr)
            .is_some_and(|entry| Arc::ptr_eq(entry, current))
        {
            by_addr.remove(&old_addr);
        }
    }
    by_addr.insert(addr, socket.clone());
    by_id.insert(id, socket.clone());
    Some(socket)
}

/// Make a single expiry decision for each current tunnel, preserving both
/// indices when the socket is busy. Old address aliases cannot remove a newer
/// tunnel's ID entry.
pub(super) async fn prune<'a>(by_addr: &AddressMap<'a>, by_id: &IdMap<'a>) -> Vec<uuid::Uuid> {
    let mut by_addr = by_addr.lock().await;
    let mut by_id = by_id.lock().await;
    let mut expired = Vec::new();
    by_id.retain(|id, socket| {
        let Some(guard) = socket.try_lock() else {
            return true;
        };
        if guard.last_seen() <= Duration::from_secs(30) {
            guard.reset_rate_limiter();
            return true;
        }
        let addr = guard.get_remote_address();
        if by_addr
            .get(&addr)
            .is_some_and(|entry| Arc::ptr_eq(entry, socket))
        {
            by_addr.remove(&addr);
        }
        expired.push(*id);
        false
    });
    by_addr.retain(|_, socket| {
        let Some(guard) = socket.try_lock() else {
            return true;
        };
        by_id
            .get(&guard.id())
            .is_some_and(|entry| Arc::ptr_eq(entry, socket))
    });
    by_addr.shrink_to_fit();
    by_id.shrink_to_fit();
    expired
}

#[cfg(test)]
mod tests {
    use super::*;

    #[actix_web::test]
    async fn older_search_finishing_last_cannot_replace_newer_tunnel() {
        let addresses = Mutex::new(HashMap::new());
        let ids = Mutex::new(HashMap::new());
        let id = uuid::Uuid::new_v4();
        let old_addr = "127.0.0.1:1000".parse().unwrap();
        let new_addr = "127.0.0.1:2000".parse().unwrap();
        let old = ManagementSocket::new_for_test(id, old_addr).await;
        let new = ManagementSocket::new_for_test(id, new_addr).await;
        let current = publish(&addresses, &ids, id, new_addr, new, 2)
            .await
            .unwrap();
        assert!(publish(&addresses, &ids, id, old_addr, old, 1)
            .await
            .is_none());
        assert!(Arc::ptr_eq(ids.lock().await.get(&id).unwrap(), &current));
        assert!(!addresses.lock().await.contains_key(&old_addr));
    }

    #[actix_web::test]
    async fn replacement_removes_old_address_and_busy_expiry_preserves_both_indices() {
        let addresses = Mutex::new(HashMap::new());
        let ids = Mutex::new(HashMap::new());
        let id = uuid::Uuid::new_v4();
        let old_addr = "127.0.0.1:1000".parse().unwrap();
        let new_addr = "127.0.0.1:2000".parse().unwrap();
        let old = ManagementSocket::new_for_test(id, old_addr).await;
        publish(&addresses, &ids, id, old_addr, old, 1)
            .await
            .unwrap();
        let new = ManagementSocket::new_for_test(id, new_addr).await;
        let current = publish(&addresses, &ids, id, new_addr, new, 2)
            .await
            .unwrap();
        assert!(!addresses.lock().await.contains_key(&old_addr));
        let mut busy = current.lock().await;
        busy.expire_for_test();
        assert!(prune(&addresses, &ids).await.is_empty());
        assert!(addresses.lock().await.contains_key(&new_addr));
        assert!(ids.lock().await.contains_key(&id));
        drop(busy);
        assert_eq!(prune(&addresses, &ids).await, vec![id]);
        assert!(addresses.lock().await.is_empty());
        assert!(ids.lock().await.is_empty());
    }
}
