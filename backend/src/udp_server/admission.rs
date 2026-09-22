use std::{
    collections::HashSet,
    net::SocketAddr,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
};
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

/// Bound discovery separately from packet handling and coalesce retransmitted
/// handshakes while a peer's database lookup / key search is still running.
pub(super) struct DiscoveryAdmission {
    slots: Arc<Semaphore>,
    peers: Arc<Mutex<HashSet<SocketAddr>>>,
}

pub(super) struct DiscoveryPermit {
    _slot: OwnedSemaphorePermit,
    peers: Arc<Mutex<HashSet<SocketAddr>>>,
    addr: SocketAddr,
    pub(super) generation: u64,
}

impl DiscoveryAdmission {
    pub(super) fn new(limit: usize) -> Self {
        Self {
            slots: Arc::new(Semaphore::new(limit)),
            peers: Arc::new(Mutex::new(HashSet::new())),
        }
    }

    pub(super) fn try_acquire(&self, addr: SocketAddr) -> Option<Arc<DiscoveryPermit>> {
        let slot = self.slots.clone().try_acquire_owned().ok()?;
        if !self.peers.lock().unwrap().insert(addr) {
            return None;
        }
        static NEXT_GENERATION: AtomicU64 = AtomicU64::new(1);
        Some(Arc::new(DiscoveryPermit {
            _slot: slot,
            peers: self.peers.clone(),
            addr,
            generation: NEXT_GENERATION.fetch_add(1, Ordering::Relaxed),
        }))
    }
}

impl Drop for DiscoveryPermit {
    fn drop(&mut self) {
        self.peers.lock().unwrap().remove(&self.addr);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coalesces_peers_and_bounds_work_until_the_worker_finishes() {
        let admission = DiscoveryAdmission::new(2);
        let first = "127.0.0.1:1".parse().unwrap();
        let second = "127.0.0.1:2".parse().unwrap();
        let third = "127.0.0.1:3".parse().unwrap();
        let permit = admission.try_acquire(first).unwrap();
        assert!(admission.try_acquire(first).is_none());
        let other = admission.try_acquire(second).unwrap();
        assert!(admission.try_acquire(third).is_none());

        // Cancelling an async caller must not free the slot while its blocking
        // worker is still running.
        let worker = permit.clone();
        drop(permit);
        assert!(admission.try_acquire(first).is_none());
        assert!(admission.try_acquire(third).is_none());
        drop(worker);
        assert!(admission.try_acquire(first).is_some());
        drop(other);
        assert!(admission.try_acquire(third).is_some());
    }
}
