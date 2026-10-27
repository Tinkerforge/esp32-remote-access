use super::*;
use smoltcp::wire::{Icmpv6Packet, Icmpv6Repr, IpProtocol, Ipv6Packet, Ipv6Repr};

fn echo_request(
    source: Ipv6Addr,
    destination: Ipv6Addr,
    ident: u16,
    seq_no: u16,
    nonce: &[u8],
) -> Vec<u8> {
    let echo = Icmpv6Repr::EchoRequest {
        ident,
        seq_no,
        data: nonce,
    };
    let header = Ipv6Repr {
        src_addr: source,
        dst_addr: destination,
        next_header: IpProtocol::Icmpv6,
        payload_len: echo.buffer_len(),
        hop_limit: 1,
    };
    let mut bytes = vec![0; header.buffer_len() + echo.buffer_len()];
    let mut packet = Ipv6Packet::new_unchecked(&mut bytes);
    header.emit(&mut packet);
    echo.emit(
        &source,
        &destination,
        &mut Icmpv6Packet::new_unchecked(packet.payload_mut()),
        &Default::default(),
    );
    bytes
}

fn assert_echo_reply(bytes: &[u8], local: Ipv6Addr, peer: Ipv6Addr, seq_no: u16, nonce: &[u8]) {
    let packet = Ipv6Packet::new_checked(bytes).unwrap();
    assert_eq!(packet.version(), 6);
    assert_eq!(packet.src_addr(), local);
    assert_eq!(packet.dst_addr(), peer);
    assert_eq!(packet.next_header(), IpProtocol::Icmpv6);
    assert_eq!(packet.total_len(), bytes.len());
    let icmp = Icmpv6Packet::new_checked(packet.payload()).unwrap();
    // Parsing verifies the checksum against the actual unicast address pair.
    assert!(matches!(
        Icmpv6Repr::parse(&local, &peer, &icmp, &Default::default()).unwrap(),
        Icmpv6Repr::EchoReply { ident: 0x4d47, seq_no: sequence, data }
            if sequence == seq_no && data == nonce
    ));
}

#[test]
fn registration_advertises_ipv6_discovery_for_management_and_user_tunnels() {
    let schema = generate_random_add_charger_schema("user", "token", String::new(), &[0; 32]);
    let multicast = IpNetwork::new(ipv6::ALL_NODES_ADDRESS.into(), 128).unwrap();
    assert_eq!(schema.charger.wg_charger_ip, multicast);
    assert_eq!(schema.charger.wg_server_ip.prefix(), 128);
    assert!(
        matches!(schema.charger.wg_server_ip, IpNetwork::V6(address) if address.ip().is_unique_local())
    );
    for key in schema.keys {
        assert_eq!(key.charger_address, multicast);
        assert_eq!(key.web_address.prefix(), 128);
        assert!(
            matches!(key.web_address, IpNetwork::V6(address) if address.ip().is_unique_local())
        );
    }
}

#[test]
fn discovery_replies_from_the_service_address_with_request_correlation() {
    let local = ipv6::random_address();
    let peer = ipv6::random_address();
    let request = echo_request(peer, ipv6::ALL_NODES_ADDRESS, 0x4d47, 42, &[7; 16]);
    let reply = ipv6::management_discovery_reply(&request, local).unwrap();
    assert_echo_reply(&reply, local, peer, 42, &[7; 16]);
}

#[test]
fn discovery_ignores_malformed_or_unrelated_packets() {
    let local = ipv6::random_address();
    let peer = ipv6::random_address();
    let request = echo_request(peer, ipv6::ALL_NODES_ADDRESS, 0x4d47, 0, &[7; 16]);
    for length in 0..request.len() {
        assert!(ipv6::management_discovery_reply(&request[..length], local).is_none());
    }
    let mut wrong_version = request.clone();
    wrong_version[0] = 0x40;
    assert!(ipv6::management_discovery_reply(&wrong_version, local).is_none());
    for offset in [4, 6, 40, 41, 42, 48] {
        let mut corrupt = request.clone();
        corrupt[offset] ^= 1;
        assert!(ipv6::management_discovery_reply(&corrupt, local).is_none());
    }
    let mut padded = request.clone();
    padded.push(0);
    assert!(ipv6::management_discovery_reply(&padded, local).is_none());
    for invalid in [
        echo_request(peer, local, 0x4d47, 0, &[7; 16]),
        echo_request(peer, ipv6::ALL_NODES_ADDRESS, 0x5757, 0, &[7; 16]),
        echo_request(peer, ipv6::ALL_NODES_ADDRESS, 0x4d47, 0, &[7; 15]),
        echo_request(peer, ipv6::ALL_NODES_ADDRESS, 0x4d47, 0, &[7; 17]),
    ] {
        assert!(ipv6::management_discovery_reply(&invalid, local).is_none());
    }
    for source in [
        Ipv6Addr::UNSPECIFIED,
        Ipv6Addr::LOCALHOST,
        ipv6::ALL_NODES_ADDRESS,
        "fe80::1".parse().unwrap(),
        "::ffff:192.0.2.1".parse().unwrap(),
        local,
    ] {
        let invalid = echo_request(source, ipv6::ALL_NODES_ADDRESS, 0x4d47, 0, &[7; 16]);
        assert!(ipv6::management_discovery_reply(&invalid, local).is_none());
    }
}

async fn receive(socket: &UdpSocket, buffer: &mut [u8]) -> usize {
    tokio::time::timeout(Duration::from_secs(2), socket.recv(buffer))
        .await
        .expect("timed out waiting for a WireGuard packet")
        .unwrap()
}

#[tokio::test]
async fn management_loop_answers_encrypted_discovery_after_relay_address_changes() {
    let charger_private = StaticSecret::from(rand::random::<[u8; 32]>());
    let charger_public = PublicKey::from(&charger_private);
    let server_private = StaticSecret::from(rand::random::<[u8; 32]>());
    let server_public = PublicKey::from(&server_private);
    let psk = rand::random::<[u8; 32]>();
    let limiter = Arc::new(boringtun::noise::rate_limiter::RateLimiter::new(
        &charger_public,
        1200,
    ));
    let tun = Tunn::new(
        charger_private,
        server_public,
        Some(psk),
        Some(PERSISTENT_KEEPALIVE_SECS),
        1,
        Some(limiter.clone()),
    );
    let mut server = Tunn::new(server_private, charger_public, Some(psk), None, 2, None);
    let socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let server_socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    socket
        .connect(server_socket.local_addr().unwrap())
        .await
        .unwrap();
    server_socket
        .connect(socket.local_addr().unwrap())
        .await
        .unwrap();
    let local = ipv6::random_address();
    let mut charger = EmuCharger {
        uuid: String::new(),
        password: String::new(),
        user_id: String::new(),
        barrier: Arc::new(Barrier::new(1)),
        socket,
        tun,
        management_address: local,
        rate_limiter: limiter,
        cleanup: Arc::new(AtomicBool::new(false)),
    };
    let mut output = [0; 2048];
    let mut network = [0; 2048];
    let TunnResult::WriteToNetwork(init) =
        charger.tun.format_handshake_initiation(&mut output, true)
    else {
        panic!("expected handshake initiation");
    };
    let TunnResult::WriteToNetwork(response) = server.decapsulate(None, init, &mut network) else {
        panic!("expected handshake response");
    };
    charger
        .handle_management_packet(response, &mut output)
        .await
        .unwrap();
    let size = receive(&server_socket, &mut network).await;
    assert!(matches!(
        server.decapsulate(None, &network[..size], &mut output),
        TunnResult::Done
    ));

    let (shutdown, mut shutdown_rx) = broadcast::channel(1);
    let poll = tokio::spawn(async move {
        charger
            .maintain_management_connection(&mut shutdown_rx)
            .await
    });
    // A restarted relay chooses a new ULA. Both probes must work on the same
    // authenticated tunnel, without pinning the previous relay's address.
    for sequence in 0..2 {
        let peer = ipv6::random_address();
        let nonce = rand::random::<[u8; 16]>();
        let request = echo_request(peer, ipv6::ALL_NODES_ADDRESS, 0x4d47, sequence, &nonce);
        let TunnResult::WriteToNetwork(encrypted) = server.encapsulate(&request, &mut output)
        else {
            panic!("expected encrypted discovery request");
        };
        server_socket.send(encrypted).await.unwrap();
        let size = receive(&server_socket, &mut network).await;
        let TunnResult::WriteToTunnelV6(reply, _) =
            server.decapsulate(None, &network[..size], &mut output)
        else {
            panic!("expected encrypted IPv6 discovery reply");
        };
        assert_echo_reply(reply, local, peer, sequence, &nonce);
    }
    shutdown.send(()).unwrap();
    assert!(
        !tokio::time::timeout(Duration::from_secs(2), poll)
            .await
            .unwrap()
            .unwrap()
    );
}
