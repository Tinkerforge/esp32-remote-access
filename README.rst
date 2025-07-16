ESP32-Remote-Access
===================

This repository contains everything needed to build and run the (**WIP**) remote access server for Tinkerforge WARP Chargers, WARP Energy Managers and ESP32-/ESP32-Ethernet-Bricks.

File structure
--------------

::

  .
  ├── backend
  ├── db_connector
  ├── docker
  ├── frontend
  └── wg-webclient


- ``backend`` The http backend server and relay server between the charger and the users browser
- ``db_connector`` A crate containing the types needed to interface with the database server
- ``docker`` Files to run and build the docker containers
- ``frontend`` Website that is served by the webserver
- ``wg-webclient`` The Wireguard implementation containing also a network stack and Http and Websocket client.

Build
-----

Prerequisites
~~~~~~~~~~~~~

- Rust toolchain: https://www.rust-lang.org/tools/install
- Node: https://nodejs.org/en
- Wasm-pack: https://github.com/rustwasm/wasm-pack
- docker: https://www.docker.com/get-started/

For the production build only docker is needed since everything is build in a container.

Developement build
~~~~~~~~~~~~~~~~~~

1. Create a ``certs`` directory in ``docker/nginx`` and place a X.509 certificate and key in pem format named ``cert.pem`` and ``key.pem`` inside.
2. Fill in the needed variables in the env variables. All needed variables are listed inside the .env.example files.
3. build the wg-package by running ``wasm-pack build`` inside ``wg-webclient``.
4. build the website by running ``npm install && npm run build`` inside ``remote-access-page``.
5. start the webserver + database by running ``docker compose -f docker-compose-dev.yml up --build`` inside ``docker``.
6. start the backend server by running ``cargo run`` inside ``backend``.

Production build
~~~~~~~~~~~~~~~~

1. Ensure that the host is accessible via a Fully Qualified Domain Name, otherwise creating a Lets Encrypt Certificate will fail.
2. Fill in the needed variables in the env file. All needed variables are listed inside the .env.example files.
3. Start everything with ``docker compose up`` inside the ``docker`` directory.
