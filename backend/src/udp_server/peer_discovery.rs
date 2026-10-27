//! Discover random management addresses with all-nodes multicast ICMPv6 echo.
//! Registered service addresses remain a bounded fallback for legacy firmware.

use smoltcp::{
    iface::SocketHandle,
    socket::icmp,
    wire::{Icmpv6Packet, Icmpv6Repr, IpAddress},
};
use std::{
    net::{IpAddr, Ipv6Addr},
    time::{Duration, Instant},
};

pub const ALL_NODES_ADDRESS: Ipv6Addr = Ipv6Addr::new(0xff02, 0, 0, 0, 0, 0, 0, 1);
pub(super) const DISCOVERY_TIMEOUT: Duration = Duration::from_secs(10);
const RETRY_INTERVAL: Duration = Duration::from_millis(500);
const DISCOVERY_IDENT: u16 = 0x4d47;

pub(super) fn random_management_address() -> Ipv6Addr {
    let mut bytes: [u8; 16] = rand::random();
    bytes[0] = 0xfd;
    bytes.into()
}

// Concrete addresses used by the existing protocol fixtures, never by sessions.
#[cfg(test)]
pub const MANAGEMENT_CHARGER_ADDRESS: Ipv6Addr =
    Ipv6Addr::new(0xfd00, 0x123, 0xffff, 0, 0, 0, 0, 2);
#[cfg(test)]
pub const MANAGEMENT_SERVER_ADDRESS: Ipv6Addr = Ipv6Addr::new(0xfd00, 0x123, 0xffff, 0, 0, 0, 0, 3);

pub(super) struct PeerDiscovery {
    pub handle: SocketHandle,
    pub fallback: Option<IpAddr>,
    pub registered_local: IpAddr,
    local: Ipv6Addr,
    deadline: Instant,
    started: bool,
    next_request: Instant,
    nonce: [u8; 16],
    outstanding: Vec<u16>,
}

impl PeerDiscovery {
    pub fn new(
        handle: SocketHandle,
        local: Ipv6Addr,
        registered_local: IpAddr,
        registered_peer: IpAddr,
        now: Instant,
    ) -> Self {
        Self {
            handle,
            registered_local,
            // Multicast discovery metadata is not a unicast service fallback.
            fallback: match registered_peer {
                IpAddr::V6(address) if address.is_multicast() => None,
                address => Some(address),
            },
            local,
            deadline: now + DISCOVERY_TIMEOUT,
            started: false,
            next_request: now,
            nonce: rand::random(),
            outstanding: Vec::new(),
        }
    }

    pub fn accepts(&self, peer: IpAddr) -> bool {
        Some(peer) == self.fallback
    }

    fn valid_reply(&self, bytes: &[u8], peer: IpAddress) -> bool {
        let IpAddress::Ipv6(peer) = peer else {
            return false;
        };
        if peer.is_unspecified()
            || peer.is_multicast()
            || peer.is_unicast_link_local()
            || peer.is_loopback()
            || peer.to_ipv4_mapped().is_some()
            || peer == self.local
        {
            return false;
        }
        let Ok(packet) = Icmpv6Packet::new_checked(bytes) else {
            return false;
        };
        matches!(Icmpv6Repr::parse(&peer, &self.local, &packet, &Default::default()),
            Ok(Icmpv6Repr::EchoReply { ident, seq_no, data })
                if ident == DISCOVERY_IDENT && self.outstanding.contains(&seq_no) && data == self.nonce)
    }

    /// `bool` records whether IPv6 was confirmed, rather than timing out.
    pub fn poll(&mut self, socket: &mut icmp::Socket<'_>, now: Instant) -> Option<(IpAddr, bool)> {
        if !self.started {
            // Waiting for WireGuard session confirmation does not consume the
            // discovery deadline before any probe can actually be sent.
            self.deadline = now + DISCOVERY_TIMEOUT;
            self.next_request = now;
            self.started = true;
        }
        while let Ok((bytes, peer)) = socket.recv() {
            if self.valid_reply(bytes, peer) {
                return Some((peer.into(), true));
            }
        }
        if now >= self.deadline {
            if let Some(peer) = self.fallback {
                return Some((peer, false));
            }
            // A discovery-only registration has no usable legacy endpoint.
            // Retry in bounded batches while the authenticated session is alive.
            log::warn!(
                "Management IPv6 discovery timed out for {}; retrying",
                self.local
            );
            self.deadline = now + DISCOVERY_TIMEOUT;
            self.next_request = now;
            self.nonce = rand::random();
            self.outstanding.clear();
        }
        if now >= self.next_request {
            let seq_no = self.outstanding.len() as u16;
            let repr = Icmpv6Repr::EchoRequest {
                ident: DISCOVERY_IDENT,
                seq_no,
                data: &self.nonce,
            };
            if let Ok(bytes) = socket.send(repr.buffer_len(), IpAddress::Ipv6(ALL_NODES_ADDRESS)) {
                repr.emit(
                    &self.local,
                    &ALL_NODES_ADDRESS,
                    &mut Icmpv6Packet::new_unchecked(bytes),
                    &Default::default(),
                );
                self.outstanding.push(seq_no);
                self.next_request = now + RETRY_INTERVAL;
            }
        }
        None
    }

    pub fn bind(socket: &mut icmp::Socket<'_>) {
        socket.bind(icmp::Endpoint::Ident(DISCOVERY_IDENT)).unwrap();
        socket.set_hop_limit(Some(1));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use smoltcp::iface::SocketSet;

    fn pending_discovery(peer: IpAddr, now: Instant) -> (PeerDiscovery, SocketSet<'static>) {
        let buffer =
            || icmp::PacketBuffer::new(vec![icmp::PacketMetadata::EMPTY; 4], vec![0; 1024]);
        let mut socket = icmp::Socket::new(buffer(), buffer());
        PeerDiscovery::bind(&mut socket);
        let mut sockets = SocketSet::new(vec![]);
        let handle = sockets.add(socket);
        let mut discovery = PeerDiscovery::new(
            handle,
            MANAGEMENT_SERVER_ADDRESS,
            "10.123.123.3".parse().unwrap(),
            peer,
            now,
        );
        discovery.nonce = [7; 16];
        (discovery, sockets)
    }

    fn discovery(peer: IpAddr, now: Instant) -> (PeerDiscovery, SocketSet<'static>) {
        let (mut discovery, mut sockets) = pending_discovery(peer, now);
        assert_eq!(
            discovery.poll(sockets.get_mut::<icmp::Socket>(discovery.handle), now),
            None
        );
        (discovery, sockets)
    }

    fn reply(ident: u16, seq_no: u16, nonce: &[u8], local: Ipv6Addr) -> Vec<u8> {
        let repr = Icmpv6Repr::EchoReply {
            ident,
            seq_no,
            data: nonce,
        };
        let mut bytes = vec![0; repr.buffer_len()];
        repr.emit(
            &MANAGEMENT_CHARGER_ADDRESS,
            &local,
            &mut Icmpv6Packet::new_unchecked(&mut bytes),
            &Default::default(),
        );
        bytes
    }

    #[test]
    fn replies_require_actual_destination_checksum_and_matching_request() {
        let (discovery, _) = discovery("10.123.123.2".parse().unwrap(), Instant::now());
        let peer = IpAddress::Ipv6(MANAGEMENT_CHARGER_ADDRESS);
        let valid = reply(
            DISCOVERY_IDENT,
            0,
            &discovery.nonce,
            MANAGEMENT_SERVER_ADDRESS,
        );
        assert!(discovery.valid_reply(&valid, peer));
        for bytes in [
            reply(
                DISCOVERY_IDENT + 1,
                0,
                &discovery.nonce,
                MANAGEMENT_SERVER_ADDRESS,
            ),
            reply(
                DISCOVERY_IDENT,
                1,
                &discovery.nonce,
                MANAGEMENT_SERVER_ADDRESS,
            ),
            reply(DISCOVERY_IDENT, 0, &[8; 16], MANAGEMENT_SERVER_ADDRESS),
            reply(DISCOVERY_IDENT, 0, &discovery.nonce, Ipv6Addr::UNSPECIFIED),
        ] {
            assert!(!discovery.valid_reply(&bytes, peer));
        }
        let mut corrupt = valid.clone();
        corrupt[2] ^= 1;
        assert!(!discovery.valid_reply(&corrupt, peer));
        assert!(!discovery.valid_reply(&valid, IpAddress::Ipv6(MANAGEMENT_SERVER_ADDRESS)));
    }

    #[test]
    fn fallback_is_bounded_and_preserves_registered_family() {
        let now = Instant::now();
        for (registered, expected) in [
            ("10.123.123.2", "10.123.123.2"),
            ("fd00:123:ffff::2", "fd00:123:ffff::2"),
            ("fe80::1", "fe80::1"),
        ] {
            let (mut discovery, mut sockets) = discovery(registered.parse().unwrap(), now);
            assert_eq!(
                discovery.poll(
                    sockets.get_mut::<icmp::Socket>(discovery.handle),
                    now + DISCOVERY_TIMEOUT - Duration::from_millis(1)
                ),
                None
            );
            assert_eq!(
                discovery.poll(
                    sockets.get_mut::<icmp::Socket>(discovery.handle),
                    now + DISCOVERY_TIMEOUT
                ),
                Some((expected.parse().unwrap(), false))
            );
        }
    }

    #[test]
    fn waiting_for_handshake_does_not_consume_the_probe_deadline() {
        let now = Instant::now();
        let (mut discovery, mut sockets) = pending_discovery("10.123.123.2".parse().unwrap(), now);
        // Start a new discovery session but defer its first poll until after
        // the original deadline, as a slow WireGuard confirmation can do.
        let confirmed_at = now + DISCOVERY_TIMEOUT * 2;
        let socket = sockets.get_mut::<icmp::Socket>(discovery.handle);
        assert_eq!(discovery.poll(socket, confirmed_at), None);
        assert_eq!(discovery.outstanding, [0]);
        assert_eq!(
            discovery.poll(
                socket,
                confirmed_at + DISCOVERY_TIMEOUT - Duration::from_millis(1)
            ),
            None
        );
        assert_eq!(
            discovery.poll(socket, confirmed_at + DISCOVERY_TIMEOUT),
            Some(("10.123.123.2".parse().unwrap(), false))
        );
    }
}
