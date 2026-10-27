use std::net::Ipv6Addr;

use smoltcp::wire::{Icmpv6Packet, Icmpv6Repr, IpProtocol, Ipv6Packet, Ipv6Repr};

pub const ALL_NODES_ADDRESS: Ipv6Addr = Ipv6Addr::new(0xff02, 0, 0, 0, 0, 0, 0, 1);
const MANAGEMENT_DISCOVERY_IDENT: u16 = 0x4d47;

pub fn random_address() -> Ipv6Addr {
    let mut bytes: [u8; 16] = rand::random();
    bytes[0] = 0xfd;
    bytes.into()
}

/// Mirror the firmware's multicast discovery filter inside the authenticated
/// management tunnel. The reply advertises the service ULA, never ff02::1.
pub fn management_discovery_reply(bytes: &[u8], local: Ipv6Addr) -> Option<Vec<u8>> {
    let packet = Ipv6Packet::new_checked(bytes).ok()?;
    let source = packet.src_addr();
    if packet.version() != 6
        || packet.total_len() != bytes.len()
        || packet.next_header() != IpProtocol::Icmpv6
        || packet.dst_addr() != ALL_NODES_ADDRESS
        || source.is_unspecified()
        || source.is_multicast()
        || source.is_unicast_link_local()
        || source.is_loopback()
        || source.to_ipv4_mapped().is_some()
        || source == local
    {
        return None;
    }

    let icmp = Icmpv6Packet::new_checked(packet.payload()).ok()?;
    if icmp.msg_code() != 0 {
        return None;
    }
    let Icmpv6Repr::EchoRequest {
        ident,
        seq_no,
        data,
    } = Icmpv6Repr::parse(&source, &ALL_NODES_ADDRESS, &icmp, &Default::default()).ok()?
    else {
        return None;
    };
    if ident != MANAGEMENT_DISCOVERY_IDENT || data.len() != 16 {
        return None;
    }

    let reply = Icmpv6Repr::EchoReply {
        ident,
        seq_no,
        data,
    };
    let header = Ipv6Repr {
        src_addr: local,
        dst_addr: source,
        next_header: IpProtocol::Icmpv6,
        payload_len: reply.buffer_len(),
        hop_limit: 64,
    };
    let mut bytes = vec![0; header.buffer_len() + reply.buffer_len()];
    let mut packet = Ipv6Packet::new_unchecked(&mut bytes);
    header.emit(&mut packet);
    reply.emit(
        &local,
        &source,
        &mut Icmpv6Packet::new_unchecked(packet.payload_mut()),
        &Default::default(),
    );
    Some(bytes)
}
