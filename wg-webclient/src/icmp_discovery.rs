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

//! ICMPv6-based peer discovery for the wg-webclient.
//!
//! When the wg-webclient (the user/browser side of the WireGuard tunnel) is
//! configured with an IPv6 `internal_peer_ip`, it does not necessarily know the
//! charger's IPv6 address ahead of time. The address it received from the
//! backend may be a "placeholder" (for example a link-local address such as
//! `fe80::1`).
//!
//! To discover the actual charger IPv6 address, the wg-webclient sends an
//! ICMPv6 echo request to the placeholder address and reads the source IPv6
//! address of the echo reply. The charger is expected to reply to these echo
//! requests (its smoltcp-based management stack is configured to do so) and
//! the reply's source IP is its real IPv6 address. The wg-webclient then uses
//! that real address as the destination for subsequent HTTP/WebSocket traffic.
//!
//! Backwards compatibility: if `internal_peer_ip` is an IPv4 address, the
//! wg-webclient falls back to using the address directly. The ICMPv6
//! discovery path is only used when the address is IPv6.

use smoltcp::{
    iface::SocketHandle,
    socket::icmp,
    wire::{Icmpv6Packet, Icmpv6Repr, IpAddress, Ipv6Address},
};

use crate::{interface::Interface, wg_device::IsUp};

/// The ICMPv6 identifier used for peer discovery. The charger is configured to
/// respond to echo requests that use this identifier (and to ignore others).
pub const DISCOVERY_IDENT: u16 = 0x5757;

/// Bind the ICMP socket to the [`DISCOVERY_IDENT`] identifier so that the
/// socket will receive echo replies with that identifier.
///
/// Must be called once after the socket is added to the interface and before
/// the first echo request is sent.
pub fn bind_discovery_socket<'a, Device>(
    iface: &mut Interface<'a, Device>,
    icmp_handle: SocketHandle,
) where
    Device: smoltcp::phy::Device + IsUp,
{
    let socket = iface.get_icmp_socket_mut(icmp_handle);
    // Binding to the discovery identifier is what makes smoltcp demultiplex
    // incoming echo replies to this socket. We do not worry about the
    // result: if the socket was already bound (e.g. due to a re-entrant
    // call from the polling closure) `bind` is a no-op.
    let _ = socket.bind(icmp::Endpoint::Ident(DISCOVERY_IDENT));
}

/// Build an ICMPv6 echo request and emit it via the given ICMP socket, from
/// `src` to `dst`.
///
/// Returns `true` if the request was queued for transmission, `false` if the
/// socket is not currently able to send.
pub fn send_echo_request<'a, Device>(
    iface: &mut Interface<'a, Device>,
    icmp_handle: SocketHandle,
    src: Ipv6Address,
    dst: Ipv6Address,
    seq_no: u16,
) -> bool
where
    Device: smoltcp::phy::Device + IsUp,
{
    let socket = iface.get_icmp_socket_mut(icmp_handle);
    if !socket.can_send() {
        return false;
    }

    // The payload is empty; the charger only needs to respond to identify
    // itself.
    let icmp_repr = Icmpv6Repr::EchoRequest {
        ident: DISCOVERY_IDENT,
        seq_no,
        data: &[],
    };

    let payload = match socket.send(icmp_repr.buffer_len(), IpAddress::Ipv6(dst)) {
        Ok(p) => p,
        Err(_) => return false,
    };

    let mut icmp_packet = Icmpv6Packet::new_unchecked(payload);
    let checksum_caps = smoltcp::phy::ChecksumCapabilities::default();
    icmp_repr.emit(&src, &dst, &mut icmp_packet, &checksum_caps);

    true
}

/// Try to receive an ICMPv6 echo reply and return the source IPv6 address of
/// the reply. Returns `None` if no reply is available.
///
/// Only replies whose ICMPv6 identifier matches `DISCOVERY_IDENT` are
/// considered, so that stray echo replies from other clients do not confuse
/// peer discovery.
pub fn try_recv_peer_address<'a, Device>(
    iface: &mut Interface<'a, Device>,
    icmp_handle: SocketHandle,
) -> Option<Ipv6Address>
where
    Device: smoltcp::phy::Device + IsUp,
{
    let socket = iface.get_icmp_socket_mut(icmp_handle);
    if !socket.can_recv() {
        return None;
    }

    let (payload, addr) = socket.recv().ok()?;
    let src = match addr {
        IpAddress::Ipv6(a) => a,
        _ => return None,
    };

    // We do not have a meaningful destination address at this point (the
    // socket was bound to the unspecified endpoint), so we pass the
    // unspecified address for parsing. The reply's source is what we care
    // about.
    let icmp_packet = Icmpv6Packet::new_checked(&payload).ok()?;
    let checksum_caps = smoltcp::phy::ChecksumCapabilities::default();
    let icmp_repr =
        Icmpv6Repr::parse(&src, &Ipv6Address::UNSPECIFIED, &icmp_packet, &checksum_caps).ok()?;

    if let Icmpv6Repr::EchoReply { ident, .. } = icmp_repr {
        if ident == DISCOVERY_IDENT {
            return Some(src);
        }
    }

    None
}

/// Parse the IP address from a `web_address` / `charger_address` style CIDR
/// string. Returns `None` if the input cannot be parsed.
///
/// The accepted formats are:
/// - `<ip>` (no prefix length, used for backwards compatibility with the
///   existing API which always passed a `/24` suffix that the client trimmed
///   by removing the last three characters)
/// - `<ip>/<prefix>`
pub fn parse_cidr_address(s: &str) -> Option<IpAddress> {
    if let Ok(cidr) = s.parse::<smoltcp::wire::IpCidr>() {
        return Some(cidr.address());
    }
    s.parse::<IpAddress>().ok()
}

/// Make a conservative "guess" at the prefix length to use when constructing
/// an `IpCidr` from a bare `IpAddress`. This is only used internally for
/// backward compatibility with the existing API that does not pass a prefix
/// length.
///
/// - IPv4: `/24` (matches the previous hard-coded value)
/// - IPv6: `/128` (single address; the peer-discovery flow only ever uses the
///   bare address)
pub fn default_prefix(addr: &IpAddress) -> u8 {
    match addr {
        IpAddress::Ipv4(_) => 24,
        IpAddress::Ipv6(_) => 128,
    }
}

/// Trait extension that exposes helpers for working with `IpAddress` values
/// coming from the keys API.
pub trait IpAddressExt {
    /// Returns `Some(addr)` if this is an IPv6 address, otherwise `None`.
    fn as_ipv6(&self) -> Option<Ipv6Address>;

    /// Returns `true` if this is an IPv4 address.
    fn is_ipv4(&self) -> bool;

    /// Returns `true` if this is an IPv6 address.
    fn is_ipv6(&self) -> bool;
}

impl IpAddressExt for IpAddress {
    fn as_ipv6(&self) -> Option<Ipv6Address> {
        match self {
            IpAddress::Ipv6(a) => Some(*a),
            _ => None,
        }
    }

    fn is_ipv4(&self) -> bool {
        matches!(self, IpAddress::Ipv4(_))
    }

    fn is_ipv6(&self) -> bool {
        matches!(self, IpAddress::Ipv6(_))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_v4_with_prefix() {
        let addr = parse_cidr_address("10.0.0.5/24").unwrap();
        assert!(addr.is_ipv4());
        assert!(!addr.is_ipv6());
    }

    #[test]
    fn parses_v4_without_prefix() {
        // Backwards compatibility: the old API passed a `/24` suffix that the
        // client trimmed. We still want to handle a bare IPv4 address.
        let addr = parse_cidr_address("10.0.0.5").unwrap();
        assert!(addr.is_ipv4());
    }

    #[test]
    fn parses_v6_with_prefix() {
        let addr = parse_cidr_address("fe80::1/64").unwrap();
        assert!(addr.is_ipv6());
        assert_eq!(addr.as_ipv6().unwrap(), Ipv6Address::new(0xfe80, 0, 0, 0, 0, 0, 0, 1));
    }

    #[test]
    fn parses_v6_without_prefix() {
        let addr = parse_cidr_address("fe80::1").unwrap();
        assert!(addr.is_ipv6());
        assert_eq!(addr.as_ipv6().unwrap(), Ipv6Address::new(0xfe80, 0, 0, 0, 0, 0, 0, 1));
    }

    #[test]
    fn rejects_invalid_address() {
        assert!(parse_cidr_address("not an address").is_none());
        assert!(parse_cidr_address("").is_none());
    }

    #[test]
    fn default_prefix_matches_address_family() {
        let v4 = IpAddress::v4(10, 0, 0, 1);
        let v6 = IpAddress::v6(0xfe80, 0, 0, 0, 0, 0, 0, 1);
        assert_eq!(default_prefix(&v4), 24);
        assert_eq!(default_prefix(&v6), 128);
    }

    #[test]
    fn as_ipv6_returns_none_for_v4() {
        let v4 = IpAddress::v4(10, 0, 0, 1);
        assert!(v4.as_ipv6().is_none());
        assert!(v4.is_ipv4());
        assert!(!v4.is_ipv6());
    }
}
