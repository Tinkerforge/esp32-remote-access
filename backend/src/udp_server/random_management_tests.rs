// Real WireGuard/smoltcp coverage for random management sessions. Included in
// socket::tests so the same encrypted peer fixtures exercise both profiles.

#[actix_web::test]
async fn independently_random_management_addresses_carry_udp_and_tcp() {
    let mut previous = None;
    for _ in 0..2 {
        let device_address = random_management_address();
        let (mut server, mut charger) = management_pair_with_registration(
            "fd00:123:ffff::3/128",
            &format!("{device_address}/128"),
            Some(("10.123.123.3/24", "10.123.123.2/24")),
        )
        .await;
        let server_address = server.local_ip;
        assert_ne!(server_address, IpAddr::V6(device_address));
        assert_ne!(server_address, IpAddr::V6(MANAGEMENT_SERVER_ADDRESS));
        if let Some((previous_server, previous_device)) = previous {
            assert_ne!(
                server_address, previous_server,
                "new relay session reused its ULA"
            );
            assert_ne!(
                device_address, previous_device,
                "new charger session reused its ULA"
            );
        }
        previous = Some((server_address, device_address));
        server.poll();
        answer_discovery_probe(&mut charger).await;
        assert!(deliver_one(&mut server).await.is_empty());
        assert_eq!(server.local_ip, server_address);
        assert_eq!(server.peer_ip, IpAddr::V6(device_address));
        assert_eq!(charger.peer_ip, server_address);
        assert!(!server.is_discovering_peer());
        assert_eq!(
            server.iface.ip_addrs(),
            &[smoltcp::wire::IpCidr::new(server_address.into(), 128)],
            "selected IPv6 management kept the temporary legacy IPv4 address"
        );
        server.encrypt_and_send_slice(b"command to random management peer");
        assert_eq!(
            deliver_one(&mut charger).await,
            b"command to random management peer"
        );
        charger.encrypt_and_send_slice(b"reply to random relay");
        assert_eq!(deliver_one(&mut server).await, b"reply to random relay");

        server.init_tcp_socket();
        server
            .get_tcp_socket()
            .unwrap()
            .listen((server_address, 8080))
            .unwrap();
        charger.init_tcp_socket();
        let handle = charger.tcp_socket.unwrap();
        charger
            .sockets
            .get_mut::<tcp::Socket>(handle)
            .connect(charger.iface.context(), (server_address, 8080), 49152)
            .unwrap();
        charger.poll();
        for _ in 0..10 {
            pump(&mut server).await;
            pump(&mut charger).await;
            if charger.get_tcp_socket().unwrap().can_send() {
                break;
            }
        }
        assert!(
            charger.get_tcp_socket().unwrap().can_send(),
            "random management TCP handshake failed"
        );
        charger
            .get_tcp_socket()
            .unwrap()
            .send_slice(b"charge log over random management addresses")
            .unwrap();
        charger.poll();
        for _ in 0..10 {
            pump(&mut server).await;
            pump(&mut charger).await;
            if server.get_tcp_socket().unwrap().can_recv() {
                break;
            }
        }
        let received = server
            .get_tcp_socket()
            .unwrap()
            .recv(|bytes| (bytes.len(), bytes.to_vec()))
            .unwrap();
        assert_eq!(received, b"charge log over random management addresses");

        server.resolve_peer(IpAddr::V6(device_address), true);
        assert_eq!(
            server.get_tcp_socket().unwrap().state(),
            tcp::State::Established,
            "same-address key rotation aborted the charge-log stream"
        );
        let server = Arc::new(Mutex::new(server));
        let receiver = ManagementSocketTCPReceiver(server.clone());
        let mut receive = Box::pin(receiver.handle_tcp_recv());
        let counter = Arc::new(WakeCounter(AtomicUsize::new(0)));
        let waker = waker_ref(&counter);
        let mut cx = Context::from_waker(&waker);
        assert!(receive.as_mut().poll(&mut cx).is_pending());
        server
            .lock()
            .await
            .resolve_peer(random_management_address().into(), true);
        assert!(
            counter.0.load(Ordering::SeqCst) > 0,
            "peer change did not wake the old TCP reader"
        );
        let Poll::Ready(TCPRecvResult::Err(error)) = receive.as_mut().poll(&mut cx) else {
            panic!("old charge-log reader remained pending after peer address changed");
        };
        assert_eq!(error.kind(), std::io::ErrorKind::ConnectionReset);
    }
}

#[actix_web::test]
async fn random_management_fallback_keeps_registered_cidr_prefixes() {
    for (local, peer) in [
        ("10.123.123.3/24", "10.123.123.2/24"),
        ("fd00:123:ffff::3/64", "fd00:123:ffff::2/64"),
    ] {
        let (mut server, mut charger) =
            management_pair_with_registration(local, peer, Some((local, peer))).await;
        server.encrypt_and_send_slice(b"legacy management profile");
        assert!(deliver_one(&mut charger).await.is_empty());
        server.poll_at(
            Instant::now()
                + super::super::peer_discovery::DISCOVERY_TIMEOUT
                + Duration::from_millis(1),
        );
        assert!(!server.is_discovering_peer());
        let cidr: IpNetwork = local.parse().unwrap();
        assert_eq!(server.iface.ip_addrs().len(), 1);
        assert_eq!(server.iface.ip_addrs()[0].address(), cidr.ip().into());
        assert_eq!(server.iface.ip_addrs()[0].prefix_len(), cidr.prefix());
        assert_eq!(
            deliver_one(&mut charger).await,
            b"legacy management profile"
        );
    }
}

#[actix_web::test]
async fn authenticated_reconnect_discovers_a_changed_management_device_address() {
    let (mut server, mut charger) = management_pair_with_registration(
        "fd00:123:ffff::3/128",
        &format!("{}/128", random_management_address()),
        Some(("10.123.123.3", "10.123.123.2")),
    )
    .await;
    server.poll();
    answer_discovery_probe(&mut charger).await;
    deliver_one(&mut server).await;
    let relay_address = server.local_ip;
    let old_device_address = server.peer_ip;
    let new_device_address: IpAddr = random_management_address().into();
    assert_ne!(old_device_address, new_device_address);
    charger.local_ip = new_device_address;
    charger.iface.update_ip_addrs(|addresses| {
        addresses.clear();
        addresses
            .push(smoltcp::wire::IpCidr::new(new_device_address.into(), 128))
            .unwrap();
    });
    charger
        .sockets
        .get_mut::<udp::Socket>(charger.sock_handle)
        .close();
    charger
        .sockets
        .get_mut::<udp::Socket>(charger.sock_handle)
        .bind((new_device_address, 12345))
        .unwrap();
    charger.device.reset();
    let mut output = [0; 2048];
    let TunnResult::WriteToNetwork(init) = charger.device.encapsulate(&[], &mut output) else {
        panic!();
    };
    assert!(server.decrypt(init).unwrap().is_empty());
    assert!(
        server.is_discovering_peer(),
        "authenticated reconnect kept a stale peer"
    );
    assert_eq!(server.local_ip, relay_address);
    // A freshly restarted charger no longer has the previous WireGuard
    // session. Drop its in-flight encrypted packets as actual firmware does.
    let mut network = [0; 2048];
    while let Ok(Ok(size)) = tokio::time::timeout(
        Duration::from_millis(5),
        charger.udp_socket.recv(&mut network),
    )
    .await
    {
        if let Err(error) = charger.decrypt(&network[..size]) {
            assert_eq!(error, "NoCurrentSession");
        }
    }
    pump(&mut server).await;
    // Consume empty keepalive confirmations without creating another reply.
    while let Ok(Ok(size)) = tokio::time::timeout(
        Duration::from_millis(5),
        charger.udp_socket.recv(&mut network),
    )
    .await
    {
        assert!(matches!(
            charger.device.decapsulate(&network[..size], &mut output),
            TunnResult::Done
        ));
    }
    server.poll_at(Instant::now() + Duration::from_millis(600));
    answer_discovery_probe(&mut charger).await;
    deliver_one(&mut server).await;
    assert_eq!(server.peer_ip, new_device_address);
    assert!(!server.is_discovering_peer());
    server.encrypt_and_send_slice(b"command after management reconnect");
    assert_eq!(
        deliver_one(&mut charger).await,
        b"command after management reconnect"
    );
}
