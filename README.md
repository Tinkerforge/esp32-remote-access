# ESP32-Remote-Access

This repository contains everything needed to build and run the (**WIP**) remote access for all Warp-Cargers and Tinkerforge ESP32-/ESP32-Ethernet-Bricks.

## File structure

```
.
├── docker
├── remote-access-page
├── wg-webclient
└── ws-udp-bridge
```

```docker:``` Files to run and build the docker containers<br>
```remote-access-page:``` Website that is served by the webserver<br>
```wg-webclient:``` The Wireguard implementation containing also a network stack and Http and Websocket client.<br>
```ws-udp-bridge``` A server to translate Websocket packets into udp packets and vice versa

## Build

### Prerequisites

Rust toolchain: https://www.rust-lang.org/tools/install<br>
Node: https://nodejs.org/en<br>
Wasm-pack: https://github.com/rustwasm/wasm-pack<br>
docker: https://www.docker.com/get-started/

### Actual build and startup

1. create a ```certs``` directory in ```docker/nginx``` and place a X.509 certificate and key in pem format named ```cert.pem``` and ```key.pem``` inside
2. build the wg-package by running ```wasm-pack build``` inside ```wg-webclient```
3. build the website by running ```npm install && npm run build``` inside ```remote-access-page```
4. adapt the (currently hardcoded) ip address of the Brick in ```ws-udp-bridge/src/main.rs```
5. start the bridge by running ```cargo run``` inside ```ws-udp-bridge```
6. start the webserver by running ```docker compose up``` inside ```docker```
