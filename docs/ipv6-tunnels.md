# Browser and management tunnels with IPv6

Public HTTPS/WebSocket, inner WireGuard traffic, and device UDP transport use
independent address families. IPv6 browsers can connect to IPv4 devices; inner
IPv6 does not enable outer IPv6 device UDP.

## Configuration

- Public HTTP defaults to `HTTP_BIND_IP=0.0.0.0`. For IPv6, use
  `HTTP_BIND_IP=::` or a dual-stack reverse proxy supporting WebSocket upgrades.
  Verify IPv4 availability when binding `::`. Ports are 8081 in debug builds
  and 443 in release builds.
- Device WireGuard UDP remains **`0.0.0.0:51820`**. Outer IPv6 requires changes
  to both the backend and firmware and is not enabled.
- Existing registrations and keys need no migration, re-registration, or
  rotation. Firmware upgrades and downgrades preserve saved addresses and keys.

## Firmware compatibility

The backend enables inner IPv6 only for recognized products at or above these
minimum versions. The policy is defined in
[`backend/src/firmware.rs`](../backend/src/firmware.rs).

| Product from the management `User-Agent` | Minimum firmware |
| --- | --- |
| `Tinkerforge-WARP_Charger` | `2.13.4` |
| `Tinkerforge-WARP2_Charger`, `Tinkerforge-WARP3_Charger`, `Tinkerforge-WARP4_Charger` | `2.13.5` |
| `ELTAKO-ELTAKO_Wallbox` | `2.13.5` |
| `Tinkerforge-WARP_Energy_Manager` | `2.9.5` |
| `Tinkerforge-WARP_Energy_Manager_2.0`, `effizient2-Smart_Energy_Broker` | `1.8.5` |

`get_key` exposes the decision as `ipv6_supported`. The browser requires an
explicit `true`; otherwise it uses registered IPv4 or `ipv4_fallback` immediately.
Without either, the connection times out. Older firmware also requires an IPv4
management address pair.

## Address selection and discovery

For supported firmware:

- IPv6 always uses random local addresses and peer discovery. Saved IPv6
  addresses are never used as service endpoints.
- IPv4 registrations enable discovery with their registered pair as fallback.
  `ff02::1` can be registered to request discovery without a service address.
- Discovery sends ICMPv6 echo requests inside the authenticated WireGuard
  tunnel from a random ULA to `ff02::1`. The peer must reply from its reachable
  service address. Only a validated reply selects IPv6.

Browser discovery keeps the registered IPv4 pair and prefix as fallback. After
three seconds without a reply, it selects IPv4; this window starts after the
handshake and first queued probe. Setup is bounded to 20 seconds and fails if no
probe was queued. Without a fallback, unanswered discovery fails after ten
seconds. For `ff02::1` registrations, `get_key` can supply the legacy slot 0–255
fallback: browser `10.123.N.3/24`, charger `10.123.N.2/32`.

The selected family stays fixed for HTTP and inner WebSocket traffic until
reconnection. Each new discovery-enabled client probes again. IPv6 management
alone does not select IPv6 user traffic.

Management discovery waits for authenticated data confirming the new WireGuard
session, then probes for up to ten seconds before using registered IPv4 addresses
as fallback. Discovery-only registrations have no legacy fallback. Legacy firmware
uses registered IPv4 immediately. Keepalives run every 15 seconds; sessions silent
for more than 30 seconds are removed by periodic cleanup.
