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

//! Multicast ICMPv6 discovery inside an authenticated one-peer WireGuard link.
//!
//! Probe ff02::1 from the browser's random ULA and learn the service address
//! from a correlated unicast echo reply. Concrete registered IPv6 services can
//! be used directly; automatic detection retains a known IPv4 fallback. Stock
//! IPv4 firmware does not answer these probes, so its fallback remains bounded.

use smoltcp::{
    iface::SocketHandle,
    socket::icmp,
    time::{Duration, Instant},
    wire::{Icmpv6Packet, Icmpv6Repr, IpAddress, IpCidr, Ipv4Address, Ipv6Address},
};

use crate::{interface::Interface, wg_device::IsUp};

pub const ALL_NODES_ADDRESS: Ipv6Address = Ipv6Address::new(0xff02, 0, 0, 0, 0, 0, 0, 1);
pub const DISCOVERY_IDENT: u16 = 0x5757;
pub const DISCOVERY_TIMEOUT: Duration = Duration::from_secs(10);
// Legacy management may itself take ten seconds to select IPv4 before the
// server can start this user tunnel. Leave room for that authenticated setup.
const AUTO_DETECTION_TIMEOUT: Duration = Duration::from_secs(20);
pub const IPV6_PROBE_TIMEOUT: Duration = Duration::from_secs(3);
const RETRY_INTERVAL: Duration = Duration::from_millis(500);

/// One discovery session. Only requests actually queued in this session may
/// resolve it. The random payload also prevents replies from an old client or
/// earlier session from satisfying a new request with the same sequence number.
pub struct PeerDiscovery {
    started: Instant,
    next_request: Instant,
    nonce: [u8; 16],
    outstanding: Vec<u16>,
    first_request: Option<Instant>,
    fallback: Option<Ipv4Address>,
    finished: bool,
    pub failed: bool,
}

impl PeerDiscovery {
    pub fn new(now: Instant, nonce: [u8; 16]) -> Self {
        Self {
            started: now,
            next_request: now,
            nonce,
            outstanding: Vec::new(),
            first_request: None,
            fallback: None,
            finished: false,
            failed: false,
        }
    }

    pub fn with_ipv4_fallback(mut self, peer: Ipv4Address) -> Self {
        self.fallback = Some(peer);
        self
    }

    pub fn poll<Device: smoltcp::phy::Device + IsUp>(
        &mut self,
        iface: &mut Interface<'_, Device>,
        handle: SocketHandle,
        local: Ipv6Address,
        now: Instant,
    ) -> Option<IpAddress> {
        if self.failed || self.finished {
            return None;
        }
        // Prefer a validated reply already received over a simultaneous timeout.
        if let Some(peer) =
            try_recv_peer_address(iface, handle, local, &self.outstanding, &self.nonce)
        {
            self.finished = true;
            return Some(IpAddress::Ipv6(peer));
        }
        let timeout = if self.fallback.is_some() { AUTO_DETECTION_TIMEOUT } else { DISCOVERY_TIMEOUT };
        let expired = now - self.started >= timeout;
        if self.first_request.is_some_and(|first| expired || now - first >= IPV6_PROBE_TIMEOUT) {
            if let Some(peer) = self.fallback {
                self.finished = true;
                return Some(IpAddress::Ipv4(peer));
            }
        }
        if expired {
            self.failed = true;
            return None;
        }
        // is_up includes the authenticated WireGuard handshake. Do not spend
        // the probe budget while the relay is still starting the device tunnel.
        if !iface.is_up() {
            return None;
        }
        if now >= self.next_request {
            let seq = self.outstanding.len() as u16;
            if send_echo_request(iface, handle, local, ALL_NODES_ADDRESS, seq, &self.nonce) {
                self.outstanding.push(seq);
                self.first_request.get_or_insert(now);
                self.next_request = now + RETRY_INTERVAL;
            }
        }
        None
    }
}

pub fn bind_discovery_socket<Device: smoltcp::phy::Device + IsUp>(
    iface: &mut Interface<'_, Device>,
    handle: SocketHandle,
) {
    let socket = iface.get_icmp_socket_mut(handle);
    socket.bind(icmp::Endpoint::Ident(DISCOVERY_IDENT)).unwrap();
    socket.set_hop_limit(Some(1));
}

pub fn send_echo_request<Device: smoltcp::phy::Device + IsUp>(
    iface: &mut Interface<'_, Device>,
    handle: SocketHandle,
    src: Ipv6Address,
    dst: Ipv6Address,
    seq_no: u16,
    data: &[u8],
) -> bool {
    let socket = iface.get_icmp_socket_mut(handle);
    let repr = Icmpv6Repr::EchoRequest {
        ident: DISCOVERY_IDENT,
        seq_no,
        data,
    };
    let Ok(payload) = socket.send(repr.buffer_len(), IpAddress::Ipv6(dst)) else {
        return false;
    };
    repr.emit(
        &src,
        &dst,
        &mut Icmpv6Packet::new_unchecked(payload),
        &Default::default(),
    );
    true
}

/// Check the checksum against the actual destination, including the IPv6 pseudo
/// header, and correlate the identifier, sequence and payload with our requests.
fn parse_peer_reply(
    payload: &[u8],
    src: Ipv6Address,
    local: Ipv6Address,
    outstanding: &[u16],
    nonce: &[u8],
) -> Option<Ipv6Address> {
    if src.is_unspecified() || src.is_multicast() || src.is_unicast_link_local()
        || src.is_loopback() || src.to_ipv4_mapped().is_some() || src == local {
        return None;
    }
    let packet = Icmpv6Packet::new_checked(payload).ok()?;
    match Icmpv6Repr::parse(&src, &local, &packet, &Default::default()).ok()? {
        Icmpv6Repr::EchoReply {
            ident,
            seq_no,
            data,
        } if ident == DISCOVERY_IDENT && outstanding.contains(&seq_no) && data == nonce => {
            Some(src)
        }
        _ => None,
    }
}

pub fn try_recv_peer_address<Device: smoltcp::phy::Device + IsUp>(
    iface: &mut Interface<'_, Device>,
    handle: SocketHandle,
    local: Ipv6Address,
    outstanding: &[u16],
    nonce: &[u8],
) -> Option<Ipv6Address> {
    let socket = iface.get_icmp_socket_mut(handle);
    while let Ok((payload, addr)) = socket.recv() {
        if let IpAddress::Ipv6(src) = addr {
            if let Some(peer) = parse_peer_reply(payload, src, local, outstanding, nonce) {
                return Some(peer);
            }
        }
    }
    None
}

/// Preserve an explicit prefix. Bare addresses retain the historical defaults:
/// IPv4 /24 and IPv6 /128. Never infer the tunnel family from the outer listener.
pub fn parse_interface_cidr(s: &str) -> Option<IpCidr> {
    if s.contains('/') {
        return s.parse().ok();
    }
    let addr: IpAddress = s.parse().ok()?;
    let prefix = match addr {
        IpAddress::Ipv4(_) => 24,
        IpAddress::Ipv6(_) => 128,
    };
    Some(IpCidr::new(addr, prefix))
}

pub fn parse_cidr_address(s: &str) -> Option<IpAddress> {
    parse_interface_cidr(s).map(|cidr| cidr.address())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    fn legacy_management_startup_does_not_consume_the_user_probe_budget() {
        let (mut iface, handle, _, _) = test_interface();
        let local = "fd00::3".parse().unwrap();
        let legacy = "10.123.1.2".parse().unwrap();
        let mut discovery = PeerDiscovery::new(Instant::from_millis(0), [7; 16]).with_ipv4_fallback(legacy);
        // The first authenticated user probe becomes possible only after the
        // server's management IPv4 fallback and user WireGuard handshake.
        assert_eq!(discovery.poll(&mut iface, handle, local, Instant::from_millis(11000)), None);
        assert!(!discovery.failed);
        assert_eq!(discovery.poll(&mut iface, handle, local, Instant::from_millis(13999)), None);
        assert_eq!(discovery.poll(&mut iface, handle, local, Instant::from_millis(14000)), Some(IpAddress::Ipv4(legacy)));
        let mut no_handshake = PeerDiscovery::new(Instant::from_millis(0), [8; 16]).with_ipv4_fallback(legacy);
        assert_eq!(no_handshake.poll(&mut iface, handle, local, Instant::from_millis(20000)), None);
        assert!(no_handshake.failed);
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    fn ipv4_fallback_is_bounded_stops_probes_and_transmits_ipv4_tcp() {
        use smoltcp::{socket::tcp, wire::{Ipv4Packet, IpProtocol}};
        let (mut iface, handle, _, tx) = test_interface();
        iface.add_ip_address("10.123.1.3/24".parse().unwrap());
        let peer = "10.123.1.2".parse().unwrap();
        let mut discovery = PeerDiscovery::new(Instant::from_millis(0), [7; 16]).with_ipv4_fallback(peer);
        let local = "fd00::3".parse().unwrap();
        for ms in (0..3000).step_by(500) {
            assert_eq!(discovery.poll(&mut iface, handle, local, Instant::from_millis(ms)), None);
            iface.poll();
        }
        let selected = discovery.poll(&mut iface, handle, local, Instant::from_millis(3000)).unwrap();
        assert_eq!(selected, IpAddress::Ipv4(peer));
        assert!(!discovery.failed);
        assert_eq!(tx.lock().unwrap().len(), 6);
        iface.remove_socket(handle);
        iface.select_address_family(selected);
        for ms in [3500, 10000, 20000] {
            assert_eq!(discovery.poll(&mut iface, handle, local, Instant::from_millis(ms)), None);
            iface.poll();
        }
        assert_eq!(tx.lock().unwrap().len(), 6, "selection did not cancel discovery");
        tx.lock().unwrap().clear();
        let tcp = tcp::Socket::new(tcp::SocketBuffer::new(vec![0; 128]), tcp::SocketBuffer::new(vec![0; 128]));
        let tcp = iface.set_socket(tcp);
        iface.connect((selected, 80), 10001, tcp).unwrap();
        iface.poll();
        let output = tx.lock().unwrap();
        let packet = Ipv4Packet::new_checked(output.front().unwrap().as_slice()).unwrap();
        assert_eq!(packet.next_header(), IpProtocol::Tcp);
        assert_eq!(packet.src_addr(), "10.123.1.3".parse::<Ipv4Address>().unwrap());
        assert_eq!(packet.dst_addr(), peer);
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    fn only_valid_contract_reply_selects_ipv6_over_ipv4_fallback() {
        use smoltcp::wire::{IpProtocol, Ipv6Packet, Ipv6Repr};
        for (source, corrupt, ipv6) in [("fd00::2", false, true), ("fe80::1", false, false), ("fd00::2", true, false)] {
            let (mut iface, handle, rx, _) = test_interface();
            let local = "fd00::3".parse().unwrap();
                let source = source.parse().unwrap();
            let legacy = "10.123.1.2".parse().unwrap();
            let nonce = [7; 16];
            let mut discovery = PeerDiscovery::new(Instant::from_millis(0), nonce).with_ipv4_fallback(legacy);
            discovery.poll(&mut iface, handle, local, Instant::from_millis(0));
            let reply = Icmpv6Repr::EchoReply { ident: DISCOVERY_IDENT, seq_no: 0, data: &nonce };
            let repr = Ipv6Repr { src_addr: source, dst_addr: local, next_header: IpProtocol::Icmpv6,
                payload_len: reply.buffer_len(), hop_limit: 64 };
            let mut bytes = vec![0; 40 + reply.buffer_len()];
            let mut packet = Ipv6Packet::new_unchecked(&mut bytes);
            repr.emit(&mut packet);
            reply.emit(&source, &local, &mut Icmpv6Packet::new_unchecked(packet.payload_mut()), &Default::default());
            if corrupt { bytes[42] ^= 1; }
            rx.lock().unwrap().push_back(bytes);
            iface.poll();
            assert_eq!(discovery.poll(&mut iface, handle, local, Instant::from_millis(3000)),
                Some(if ipv6 { IpAddress::Ipv6(source) } else { IpAddress::Ipv4(legacy) }));
            assert!(!discovery.failed);
        }
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    fn preserves_prefixes_and_bare_address_defaults() {
        for (input, expected) in [
            ("fd00::3/64", "fd00::3/64"),
            ("fd00::3/128", "fd00::3/128"),
            ("10.123.1.3/24", "10.123.1.3/24"),
            ("10.123.1.3", "10.123.1.3/24"),
            ("fd00::3", "fd00::3/128"),
        ] {
            assert_eq!(parse_interface_cidr(input).unwrap().to_string(), expected);
        }
        for input in ["", "garbage", "fd00::3/129", "10.0.0.3/33"] {
            assert!(parse_interface_cidr(input).is_none());
        }
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    fn reply_checksum_and_request_correlation() {
        let local = "fd00::3".parse().unwrap();
        let peer = "fd00::2".parse().unwrap();
        let nonce = b"discovery-session";
        let repr = Icmpv6Repr::EchoReply {
            ident: DISCOVERY_IDENT,
            seq_no: 7,
            data: nonce,
        };
        let mut bytes = vec![0; repr.buffer_len()];
        repr.emit(
            &peer,
            &local,
            &mut Icmpv6Packet::new_unchecked(&mut bytes),
            &Default::default(),
        );
        assert_eq!(
            parse_peer_reply(&bytes, peer, local, &[7], nonce),
            Some(peer)
        );
        assert_eq!(
            parse_peer_reply(&bytes, peer, Ipv6Address::UNSPECIFIED, &[7], nonce),
            None
        );
        assert_eq!(parse_peer_reply(&bytes, peer, local, &[8], nonce), None);
        assert_eq!(
            parse_peer_reply(&bytes, peer, local, &[7], b"old-session"),
            None
        );
        let wrong_ident = Icmpv6Repr::EchoReply {
            ident: DISCOVERY_IDENT + 1,
            seq_no: 7,
            data: nonce,
        };
        wrong_ident.emit(
            &peer,
            &local,
            &mut Icmpv6Packet::new_unchecked(&mut bytes),
            &Default::default(),
        );
        assert_eq!(parse_peer_reply(&bytes, peer, local, &[7], nonce), None);
        repr.emit(
            &peer,
            &local,
            &mut Icmpv6Packet::new_unchecked(&mut bytes),
            &Default::default(),
        );
        bytes[2] ^= 1;
        assert_eq!(parse_peer_reply(&bytes, peer, local, &[7], nonce), None);
    }
    fn test_interface() -> (
        Interface<'static, crate::tests::LocalDevice>,
        SocketHandle,
        std::sync::Arc<std::sync::Mutex<std::collections::VecDeque<Vec<u8>>>>,
        std::sync::Arc<std::sync::Mutex<std::collections::VecDeque<Vec<u8>>>>,
    ) {
        use std::collections::VecDeque;
        use std::sync::{Arc, Mutex};
        let rx = Arc::new(Mutex::new(VecDeque::new()));
        let tx = Arc::new(Mutex::new(VecDeque::new()));
        let mut iface = Interface::new(
            crate::tests::LocalDevice::new(rx.clone(), tx.clone()),
            "fd00::3/128".parse().unwrap(),
        );
        let buffer =
            || icmp::PacketBuffer::new(vec![icmp::PacketMetadata::EMPTY; 4], vec![0; 1024]);
        let handle = iface.set_icmp_socket(buffer(), buffer());
        bind_discovery_socket(&mut iface, handle);
        (iface, handle, rx, tx)
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    fn receives_reply_for_concrete_local_address_and_rejects_corruption() {
        use smoltcp::wire::{IpProtocol, Ipv6Packet, Ipv6Repr};
        let (mut iface, handle, rx, _) = test_interface();
        let local = "fd00::3".parse().unwrap();
        let peer = "fd00::2".parse().unwrap();
        let nonce = [42; 16];
        let reply = Icmpv6Repr::EchoReply {
            ident: DISCOVERY_IDENT,
            seq_no: 9,
            data: &nonce,
        };
        let repr = Ipv6Repr {
            src_addr: peer,
            dst_addr: local,
            next_header: IpProtocol::Icmpv6,
            payload_len: reply.buffer_len(),
            hop_limit: 64,
        };
        let mut bytes = vec![0; 40 + reply.buffer_len()];
        let mut packet = Ipv6Packet::new_unchecked(&mut bytes);
        repr.emit(&mut packet);
        reply.emit(
            &peer,
            &local,
            &mut Icmpv6Packet::new_unchecked(packet.payload_mut()),
            &Default::default(),
        );
        rx.lock().unwrap().push_back(bytes.clone());
        iface.poll();
        assert_eq!(
            try_recv_peer_address(&mut iface, handle, local, &[9], &nonce),
            Some(peer)
        );
        bytes[42] ^= 1;
        rx.lock().unwrap().push_back(bytes);
        iface.poll();
        assert_eq!(
            try_recv_peer_address(&mut iface, handle, local, &[9], &nonce),
            None
        );
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    fn discovery_is_bounded_and_medium_ip_transmits_multicast() {
        let (mut iface, handle, _, tx) = test_interface();
        let mut discovery = PeerDiscovery::new(Instant::from_millis(0), [7; 16]);
        let local = "fd00::3".parse().unwrap();
        for milliseconds in (0..10_000).step_by(100) {
            assert_eq!(
                discovery.poll(
                    &mut iface,
                    handle,
                    local,
                    Instant::from_millis(milliseconds)
                ),
                None
            );
            iface.poll();
        }
        assert_eq!(
            tx.lock().unwrap().len(),
            20,
            "Medium::Ip must emit multicast IP packets through the tunnel without a gateway route"
        );
        for (seq_no, bytes) in tx.lock().unwrap().iter().enumerate() {
            let packet = smoltcp::wire::Ipv6Packet::new_checked(bytes).unwrap();
            assert_eq!(packet.src_addr(), local);
            assert_eq!(packet.dst_addr(), ALL_NODES_ADDRESS);
            assert_eq!(packet.hop_limit(), 1);
            assert_eq!(packet.next_header(), smoltcp::wire::IpProtocol::Icmpv6);
            let echo = Icmpv6Packet::new_checked(packet.payload()).unwrap();
            assert_eq!(
                Icmpv6Repr::parse(&local, &ALL_NODES_ADDRESS, &echo, &Default::default()),
                Ok(Icmpv6Repr::EchoRequest {
                    ident: DISCOVERY_IDENT,
                    seq_no: seq_no as u16,
                    data: &[7; 16],
                })
            );
        }
        discovery.poll(
            &mut iface,
            handle,
            local,
            Instant::from_millis(10_000),
        );
        assert!(discovery.failed);
        for milliseconds in [10_500, 20_000, 100_000] {
            discovery.poll(
                &mut iface,
                handle,
                local,
                Instant::from_millis(milliseconds),
            );
            iface.poll();
        }
        assert_eq!(
            tx.lock().unwrap().len(),
            20,
            "timed out discovery kept retrying"
        );
    }
}
