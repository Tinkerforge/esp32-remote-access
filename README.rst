ESP32-Remote-Access
===================

Remote access server and browser client for Tinkerforge WARP Chargers, WARP
Energy Managers and ESP32-/ESP32-Ethernet-Bricks. The service powers
my.warp-charger.com.

The Rust backend serves the website and HTTPS API, maintains device management
tunnels, and relays encrypted WireGuard packets between devices and browsers.
The browser runs its own WireGuard and TCP/IP stack in WebAssembly; no browser
extension or local VPN installation is needed.

.. BEGIN WARP REPOSITORIES (managed block, generated from esp32-firmware/repo_overview.rst - do not edit by hand, run update_repo_overview.py instead)

WARP Repositories
-----------------

This is an overview of all repositories that make up the WARP Charger and
WARP Energy Manager ecosystem.

Please report **any** issue concerning WARP hard- or software in the
`esp32-firmware`_ repository, no matter which repository it originates from.

Software
~~~~~~~~

- `esp32-firmware`_ - Source code of the ESP32 firmware shared between all WARP Chargers and Energy Managers

Libraries used by the firmware:

- `tfjson`_ - SAX style JSON serializer and deserializer
- `tfnetwork`_ - Modbus TCP server and client implementation
- `tfocpp`_ - OCPP 1.6 implementation
- `tftools`_ - Miscellaneous tools and helpers

Remote access:

- `esp32-remote-access`_ - Source code of the my.warp-charger.com remote access server

Documentation, tools and integrations:

- `warp-charger`_ - Source code of (docs.)warp-charger.com and the printed manual, released firmwares, datasheets and documents, as well as some tools and hardware design files
- `api.warp-charger.com`_ - Serves APIs that are used by WARP Chargers to obtain relevant public information like day ahead prices
- `vislog.warp-charger.com`_ - Visualizes WARP Charger logs and EVSE debug protocols
- `dbus-warp-charger`_ - Integrates WARP Chargers into a Victron Energy Venus OS device (e.g. Cerbo GX)

WARP Charger Hardware
~~~~~~~~~~~~~~~~~~~~~~

WARP4 Charger
"""""""""""""

*Current charger generation.*

- `warp-esp32-ethernet-v2-brick`_ - Hardware design files of the WARP ESP32 Ethernet Brick 2.0
- `warp-esp32-ethernet-v2-co-bricklet`_ - Firmware source code of the WARP ESP32 Ethernet 2.0 Co Bricklet (co-processor)
- `evse-v4-bricklet`_ - Hardware design files of the EVSE 4.0 Bricklet (firmware shared with the EVSE 2.0 Bricklet)
- `nfc-bricklet`_ - Firmware source code and hardware design files of the NFC Bricklet

WARP3 Charger
"""""""""""""

- `warp-esp32-ethernet-brick`_ - Hardware design files of the WARP ESP32 Ethernet Brick
- `evse-v3-bricklet`_ - Hardware design files of the EVSE 3.0 Bricklet (firmware shared with the EVSE 2.0 Bricklet)
- `nfc-bricklet`_ - Firmware source code and hardware design files of the NFC Bricklet

WARP2 Charger
"""""""""""""

- `esp32-ethernet-brick`_ - Hardware design files of the ESP32 Ethernet Brick
- `evse-v2-bricklet`_ - Firmware source code and hardware design files of the EVSE 2.0 Bricklet
- `nfc-bricklet`_ - Firmware source code and hardware design files of the NFC Bricklet

WARP1 Charger
"""""""""""""

- `esp32-brick`_ - Hardware design files of the ESP32 Brick
- `evse-bricklet`_ - Firmware source code and hardware design files of the EVSE Bricklet
- `rs485-bricklet`_ - Firmware source code and hardware design files of the RS485 Bricklet

WARP Energy Manager Hardware
~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

WARP Energy Manager 2.0
"""""""""""""""""""""""

*Current energy manager generation.*

- `esp32-ethernet-brick`_ - Hardware design files of the ESP32 Ethernet Brick
- `warp-energy-manager-v2-bricklet`_ - Firmware source code and hardware design files of the WARP Energy Manager 2.0 Bricklet
- `warp-front-panel-bricklet`_ - Firmware source code and hardware design files of the WARP Front Panel Bricklet

WARP Energy Manager 1.0
"""""""""""""""""""""""

- `esp32-ethernet-brick`_ - Hardware design files of the ESP32 Ethernet Brick
- `warp-energy-manager-bricklet`_ - Firmware source code and hardware design files of the WARP Energy Manager Bricklet

Forked/patched projects
~~~~~~~~~~~~~~~~~~~~~~~~~

- `arduino-esp32`_
- `esp32-arduino-libs`_
- `WireGuard-ESP32-Arduino`_

.. Link targets: One definition per repository, keep in sync with the lists above.

.. _esp32-firmware: https://github.com/Tinkerforge/esp32-firmware
.. _tfjson: https://github.com/Tinkerforge/tfjson
.. _tfnetwork: https://github.com/Tinkerforge/tfnetwork
.. _tfocpp: https://github.com/Tinkerforge/tfocpp
.. _tftools: https://github.com/Tinkerforge/tftools
.. _esp32-remote-access: https://github.com/Tinkerforge/esp32-remote-access
.. _warp-charger: https://github.com/Tinkerforge/warp-charger
.. _api.warp-charger.com: https://github.com/Tinkerforge/api.warp-charger.com
.. _vislog.warp-charger.com: https://github.com/Tinkerforge/vislog.warp-charger.com
.. _dbus-warp-charger: https://github.com/Tinkerforge/dbus-warp-charger
.. _warp-esp32-ethernet-v2-brick: https://github.com/Tinkerforge/warp-esp32-ethernet-v2-brick
.. _warp-esp32-ethernet-v2-co-bricklet: https://github.com/Tinkerforge/warp-esp32-ethernet-v2-co-bricklet
.. _evse-v4-bricklet: https://github.com/Tinkerforge/evse-v4-bricklet
.. _nfc-bricklet: https://github.com/Tinkerforge/nfc-bricklet
.. _warp-esp32-ethernet-brick: https://github.com/Tinkerforge/warp-esp32-ethernet-brick
.. _evse-v3-bricklet: https://github.com/Tinkerforge/evse-v3-bricklet
.. _esp32-ethernet-brick: https://github.com/Tinkerforge/esp32-ethernet-brick
.. _evse-v2-bricklet: https://github.com/Tinkerforge/evse-v2-bricklet
.. _esp32-brick: https://github.com/Tinkerforge/esp32-brick
.. _evse-bricklet: https://github.com/Tinkerforge/evse-bricklet
.. _rs485-bricklet: https://github.com/Tinkerforge/rs485-bricklet
.. _warp-energy-manager-v2-bricklet: https://github.com/Tinkerforge/warp-energy-manager-v2-bricklet
.. _warp-front-panel-bricklet: https://github.com/Tinkerforge/warp-front-panel-bricklet
.. _warp-energy-manager-bricklet: https://github.com/Tinkerforge/warp-energy-manager-bricklet
.. _arduino-esp32: https://github.com/Tinkerforge/arduino-esp32
.. _esp32-arduino-libs: https://github.com/Tinkerforge/esp32-arduino-libs
.. _WireGuard-ESP32-Arduino: https://github.com/Tinkerforge/WireGuard-ESP32-Arduino

.. END WARP REPOSITORIES

Documentation
-------------

- `Architecture and database relationships <entity_relations.md>`_
- `Account, registration and connection flows <sequence.md>`_
- `Data formats, encryption and authorization tokens <data_type_definitions.md>`_
- `API reference and WebSocket messages <api.md>`_
- `Tests and development checks <testing.md>`_
- `Deployment, backups and operational limitations <operations.md>`_
- `iOS wrapper preparation <ios_preparation.md>`_

Repository layout
-----------------

::

  .
  ├── ansible        Deployment/update playbooks
  ├── backend        Actix Web HTTPS server and UDP relay
  ├── db_connector   Diesel models, schema, pool and embedded migrations
  ├── docker         Compose files, Dockerfiles and legacy nginx configuration
  ├── frontend       Preact/TypeScript application, workers and tests
  ├── scripts        Database backup and Ed25519 signing utilities
  ├── tun            Experimental Linux TUN client
  ├── warp_emu       Device management/load-test emulator
  └── wg-webclient   Rust/WebAssembly WireGuard, TCP/IP, HTTP and WebSocket client

The Rust crates have separate manifests and lockfiles; there is no root Cargo
workspace. Run Cargo commands from the relevant crate directory.

Local development
-----------------

Prerequisites
~~~~~~~~~~~~~

- A current stable Rust toolchain, including the ``wasm32-unknown-unknown`` target.
- Node.js 24 (used by CI and the Docker frontend builds) and npm.
- ``wasm-pack`` and a C compiler; the container WASM builds install Clang.
- PostgreSQL, or Docker with the Compose v2 plugin to run the supplied database.
- Native build dependencies for Rust's PostgreSQL/OpenSSL dependencies. On
  Debian/Ubuntu these include ``build-essential``, ``pkg-config``, ``libpq-dev``
  and ``libssl-dev``.
- A trusted TLS certificate and private key for the development hostname.
- SMTP credentials for registration, verification and recovery email flows.

Build and start
~~~~~~~~~~~~~~~

Commands below start in the repository root unless a different directory is
shown. Copy example files only if you have not already configured them.

1. Create configuration files and fill in their values::

     cp backend/.env.example backend/.env
     cp docker/.env.example docker/.env
     cp frontend/.env.local.example frontend/.env.local

   For local PostgreSQL, set ``PG_PASS``, ``POSTGRES_USER`` and ``POSTGRES_DB``
   in ``docker/.env``. Set the matching backend ``DATABASE_URL`` to
   ``postgres://USER:PASSWORD@localhost:5432/DATABASE``. URL-encode special
   characters in credentials.

2. Clone the ``warp-charger`` repository beside this checkout if you want the
   server to supply matching firmware web interfaces::

     git clone https://github.com/Tinkerforge/warp-charger.git ../warp-charger

   When starting the backend from ``backend/``, set
   ``WARP_CHARGER_GIT_PATH=../../warp-charger``. If a matching HTML file is
   missing, the service worker falls back to fetching the interface from the
   device through the tunnel.

3. Configure ``backend/.env``:

   - Set ``JWT_SECRET`` to a long random value.
   - Set the SMTP and sender variables to usable values; they are read at startup.
   - Set ``STATIC_FILES_DIR=../frontend/dist``.
   - Set ``FRONTEND_URL=https://YOUR-DEVELOPMENT-HOST:8081``.
   - Set ``TLS_CERT_PATH`` and ``TLS_KEY_PATH`` to readable PEM certificate and
     private-key files. Paths are relative to ``backend/`` unless absolute.
   - Set ``FORWARD_HOST`` to the IPv4 address of your UDP forwarding host, or
     ``0.0.0.0`` if there is none. This variable is used during tunnel discovery.
   - Set ``BRAND=warp`` or ``BRAND=seb`` as appropriate.

   Install/trust the certificate's issuing CA in the browser and configure the
   device to trust it when testing hardware. TLS is required even in debug
   builds. HTTPS also provides the context needed by the service worker.

4. Build the local WASM dependency before installing frontend packages::

     cd wg-webclient
     rustup target add wasm32-unknown-unknown
     wasm-pack build --target bundler
     cd ../frontend
     npm ci
     BRAND=warp npm run build
     cd ..

   Use ``BRAND=seb`` for a SEB build. Vite configuration reads
   ``process.env.BRAND`` directly, so export it or prefix the build command;
   putting it only in ``.env.local`` does not select all build-time branding.
   The build generates ``frontend/dist/sw.js`` and ``version.json``.
   Use the ``bundler`` WASM target (also wasm-pack's default), matching the Docker
   builds and the worker's static imports. The ``build:wasm``/``typecheck`` npm
   scripts currently generate the ``web`` target, which requires explicit WASM
   initialization that the worker does not perform. Rebuild with the bundler
   target before building/running the frontend after those scripts.

5. Start the local database (and, optionally, Adminer)::

     cd docker
     docker compose -f docker-compose-dev.yml up -d postgres
     # Optional database UI on http://localhost:8080:
     docker compose -f docker-compose-dev.yml up -d adminer
     cd ../backend
     cargo run

   The backend applies embedded database migrations automatically before
   serving requests. It listens on HTTPS port **8081** in debug builds and UDP
   port **51820**. Open the configured ``FRONTEND_URL`` in your browser.

``docker-compose-dev.yml`` starts only PostgreSQL and Adminer. It does not
start nginx, build the frontend or obtain certificates. Its database has no
persistent volume; removing the container loses its data.

For full remote-access testing, use the built frontend served by the backend.
``npm run dev`` starts plain Vite; the current Vite configuration has no API
proxy or HTTPS setup, and its service-worker build plugin runs only during
``npm run build``. Rebuild the frontend after changes and reload the browser.

Configuration reference
~~~~~~~~~~~~~~~~~~~~~~~

The backend loads ``.env`` from its working directory (or a parent directory).
Compose reads ``docker/.env`` for interpolation; it passes only variables
listed in the service's ``environment`` section into the container.

.. list-table:: Backend environment
   :header-rows: 1
   :widths: 35 65

   * - Variable
     - Purpose
   * - ``DATABASE_URL``
     - PostgreSQL connection string; required, including for migrations.
   * - ``JWT_SECRET``
     - Signs access and refresh JWTs; required.
   * - ``EMAIL_USER``, ``EMAIL_PASS``, ``EMAIL_RELAY``, ``EMAIL_RELAY_PORT``
     - SMTP credentials, relay and port; required. The code uses
       ``SmtpTransport::relay`` (implicit TLS), not a configurable plaintext or
       STARTTLS transport.
   * - ``SENDER_EMAIL``, ``SENDER_NAME``
     - Outgoing email sender; required.
   * - ``FRONTEND_URL``
     - Public HTTPS origin, including the port when needed, for email links;
       required. Avoid a trailing slash.
   * - ``STATIC_FILES_DIR``
     - Built frontend directory; required locally, ``/public`` in the production
       image and ``/frontend`` in staging Compose.
   * - ``TLS_CERT_PATH``, ``TLS_KEY_PATH``
     - PEM certificate chain and private key; required.
   * - ``FORWARD_HOST``
     - IPv4 forwarding host used by UDP discovery. Set explicitly for reliable
       discovery outside known source-IP/subnet candidates.
   * - ``WARP_CHARGER_GIT_PATH``
     - Debug-only path to the ``warp-charger`` checkout; defaults to
       ``warp-charger``. Release builds use ``/static`` instead.
   * - ``BRAND``
     - Runtime email branding, ``warp`` (default) or ``seb``. Frontend branding
       is chosen separately when building the assets.
   * - ``SERVER_NAME``, ``MONITORING_EMAIL``
     - Optional daily statistics email; enabled when both variables are present.
       Unset either to disable it; empty strings still count as present.
   * - ``OPTIONAL_SERVICE_AUTH_TOKEN``
     - Optional string returned verbatim by the public
       ``GET /api/auth/service_token`` endpoint. Unset/empty returns HTTP 500.

Production and staging
----------------------

``docker/docker-compose.yml`` runs a published backend/frontend image,
PostgreSQL 16 and Adminer. The image serves HTTPS on **443** and relays UDP on
**51820**. There is no nginx or Certbot service in this production Compose file.
Provide certificates and arrange renewal separately.

The image name is
``tinkerforge/esp32-remote-access:${IMAGE_TAG}-${BRAND}``. For example,
``IMAGE_TAG=latest`` and ``BRAND=warp`` select ``latest-warp``; do not add the
brand suffix to ``IMAGE_TAG``. Tag pushes build and publish both brand variants
through ``.github/workflows/docker-publish.yml``.

To build an image locally from the repository root::

  docker build -f docker/production/Dockerfile --build-arg BRAND=warp -t esp32-remote-access:local-warp .

See `operations.md <operations.md>`_ before starting a deployment. It covers
certificate and firmware mounts, logs, Compose overrides, staging limitations
and the update/backup scripts.

API documentation
-----------------

Run the separate documentation server from ``backend/``::

  cargo run --bin api_docs

Swagger UI is served over HTTP at ``http://localhost:12345/`` and its OpenAPI
JSON at ``http://localhost:12345/api-docs/openapi.json``. This binary does not
start the application, connect to the database or host the actual API. It binds
to all interfaces. Application routes are mounted under ``/api``; generated
OpenAPI paths omit that prefix. See `api.md <api.md>`_ for the current route map
and legacy aliases, and `testing.md <testing.md>`_ for regenerating API types.

Auxiliary tools
---------------

``tun`` is an experimental Linux client. Build it with ``cargo build --release``
inside ``tun/``. Its CLI accepts ``--host`` (default ``my.warp-charger.com``),
``--email`` and ``--password`` or ``HOST``, ``EMAIL`` and ``PASSWORD`` environment
variables, followed by ``list`` or ``connect DEVICE_UUID``. Connecting requires
``/dev/net/tun`` and is intended to run through ``sudo``: after creating the
interface it reads ``SUDO_UID``/``SUDO_GID`` and drops privileges. The current
``list`` command still expects an HTTP JSON device list and is incompatible
with the server's WebSocket device-list endpoint. The CLI currently accepts
invalid TLS certificates unconditionally.

``warp_emu`` registers synthetic devices and maintains management tunnels for
load testing. Its arguments are ``TARGET_HOST AUTH_TOKEN --num N`` (default
``N=10``); use a reusable authorization token for multiple devices. The
positional host selects the UDP destination, while HTTPS registration,
management and cleanup still use the source constant ``HOST`` in
``warp_emu/src/main.rs``. It does not emulate a complete device web interface or
produce properly sealed browser keys. Ctrl+C starts device cleanup; a second
Ctrl+C exits immediately.

``scripts/sign_string.py`` signs bytes with libsodium Ed25519 combined mode.
Run ``python3 scripts/sign_string.py --help`` for key-file, environment and
KeePassXC options. It requires a **64-byte** secret key and emits a length line
before the Base64-encoded ``signature || message``; consumers must account for
both output lines.

License
-------

See `LICENSE.md <LICENSE.md>`_ for the license text included in this repository.
