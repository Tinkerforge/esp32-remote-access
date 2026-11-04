# Deployment and operations

See [README.rst](README.rst) for local development and the backend environment
reference. This document describes the checked-in Compose files and scripts;
site-specific inventory, credentials and certificates are supplied separately.

## Production image

[Production Compose](docker/docker-compose.yml) runs a published image containing
the Rust backend and built frontend, PostgreSQL 16 with a persistent bind mount,
and Adminer. The backend terminates TLS itself on TCP 443 and listens on UDP
51820. Neither nginx nor Certbot is part of this stack.

Paths below are relative to the repository root. Compose resolves the bind-mount
paths in its file relative to `docker/docker-compose.yml`:

| Host path | Container path | Use |
| --- | --- | --- |
| `docker/.env` | Passed selectively as environment variables | Compose/application configuration |
| `docker/certs/` | `/certs` (read-only) | Certificate chain and private key |
| `docker/warp-charger/firmwares/static_html/` | `/static` | Firmware web-interface files |
| `docker/postgres/data/` | `/var/lib/postgresql/data` | Persistent database |
| `docker/logs/` | `/logs/backend` in the checked-in file | See the log-path correction below |

Copy `docker/.env.example` to `docker/.env` if needed, then configure:

- `IMAGE_TAG=latest` (or a release tag without a brand suffix) and `BRAND=warp`
  or `seb`. The image reference appends `-${BRAND}` itself. Runtime branding
  should match the frontend brand built into the selected image.
- `PG_PASS`, `POSTGRES_USER` and `POSTGRES_DB`, and a matching `DATABASE_URL`
  using host `postgres`, for example
  `postgres://USER:URL_ENCODED_PASSWORD@postgres:5432/DATABASE`.
- The required backend JWT, SMTP, sender, frontend URL and TLS settings listed
  in the README. Certificate paths refer to the **container**, for example
  `/certs/live/remote.example.com/fullchain.pem` and the matching `privkey.pem`.
- `FORWARD_HOST`, using the IPv4 forwarding host or `0.0.0.0` when none is used.
- `ADMINER_IP`, such as `127.0.0.1`, for the host interface on which to publish
  Adminer port 8080.
- A valid `EXTRA_HOSTS` entry. Compose always interpolates one entry; an empty
  value is not a usable host mapping. For example,
  `host.docker.internal:host-gateway` can be used with Docker's host gateway.
- `UID` and `GID` if the backend should run as a different numeric user than
  the default `1000:1000`. Ensure that user can read the certificates and write
  logs. Monitoring variables are optional in the backend, but production
  Compose always passes them; use an override to omit them if disabling mail.

The firmware checkout belongs **inside `docker/`** for production Compose:

```sh
git clone https://github.com/Tinkerforge/warp-charger.git docker/warp-charger
mkdir -p docker/certs docker/logs
```

Provide a valid certificate chain and key before starting the backend. The
production stack does not obtain or renew them. The backend loads certificates
at startup; restart it after replacing renewed certificate files. Permit inbound
TCP 443 and UDP 51820 on the host. Port 80 is needed only if the chosen external
certificate provisioning method requires it.

### Log mount and health check

Release builds create `/logs/backend-YYYY-MM-DD-HH.log` at startup. The supplied
production mount targets `/logs/backend`, so it does **not** persist these files.
Mount the host log directory at `/logs`, for example in an operator-owned
`docker/compose.local.yml`:

```yaml
services:
  backend:
    volumes:
      - ./logs:/logs
    healthcheck:
      disable: true
```

Disabling the supplied health check in this example avoids a check that cannot
work in the current production image: it invokes `curl`, which that image does
not install, and probes `GET /api/management`, while management is a `PUT`
endpoint. Use an appropriate external availability check or provide a validated
replacement. An ordinary `GET /api/management` is not a functional health API.
The release log file is selected at startup; the backend does not implement
hourly rotation simply because the filename contains an hour.

After preparing configuration, mounts and the override, run from the repository
root:

```sh
docker compose -f docker/docker-compose.yml -f docker/compose.local.yml config --quiet
docker compose -f docker/docker-compose.yml -f docker/compose.local.yml pull
docker compose -f docker/docker-compose.yml -f docker/compose.local.yml up -d
docker compose -f docker/docker-compose.yml -f docker/compose.local.yml logs -f backend
```

The backend applies embedded migrations at startup. Back up the database before
upgrades. Replacing an image with an earlier tag does not revert migrations.

## Staging stack and nginx files

[docker-compose-staging.yml](docker/docker-compose-staging.yml) builds the backend,
WASM package and frontend from bind-mounted sources. It uses the sibling
`warp-charger` checkout (`../../warp-charger` relative to `docker/`), serves the
frontend from `/frontend`, and uses certificates under `docker/nginx/certs/`.
Its one-shot Certbot service obtains a certificate using standalone HTTP on port
80. It does not schedule renewal.

This file is a separate stack, not an override for production. It needs an
explicit `-f docker-compose-staging.yml` selection. It also needs adjustments
before use: the release backend requires a writable `/logs` directory, and both
the backend and standalone Certbot publish host port 80. Resolve the overlapping
port bindings, add the log mount, and replace the management health probe. Its
backend environment list does not forward `OPTIONAL_SERVICE_AUTH_TOKEN`.

The files under `docker/nginx/` are legacy proxy configuration and are not used
by any current Compose service. They proxy plain HTTP and strip `/api/`, whereas
the current backend expects HTTPS and retains the `/api` prefix. Do not use
them as a current deployment recipe without adapting them.

## Backups

For an explicit dump of the configured database, run from the repository root:

```sh
docker compose -f docker/docker-compose.yml exec -T postgres \
  sh -c 'pg_dump -U "$POSTGRES_USER" -d "$POSTGRES_DB"' > backup.sql
```

To restore a dump into a prepared empty database:

```sh
docker compose -f docker/docker-compose.yml exec -T postgres \
  sh -c 'psql -v ON_ERROR_STOP=1 -U "$POSTGRES_USER" -d "$POSTGRES_DB"' < backup.sql
```

Keep backups outside the database volume and check that a dump can be restored.

[scripts/backup_database.sh](scripts/backup_database.sh) is tailored to the
Ansible production layout: it reads `POSTGRES_USER` from `../.env` relative to
the script, dumps the container named `postgres` into `~/backups`, and keeps the
most recent **364 files** (despite comments saying 365). It does not specify
`-d`, so PostgreSQL defaults to the database named after the database user.
Check that this is the intended database. In a repository checkout the usual
configuration is `docker/.env`, which does not match the script's expected
root `.env` path.

## Update playbooks

The playbooks require an operator-supplied Ansible inventory and the
`community.docker` collection; none of the examples below should be interpreted
as an instruction to deploy automatically.

- `production_update_playbook.yml` targets `production`; it prompts for a tag,
  compares `.env` keys with that tag's Docker example, updates `IMAGE_TAG`,
  downloads Compose into `~/remote_access`, updates its `warp-charger` checkout,
  pulls/recreates services and installs the backup script plus a 03:00 daily
  cron job. Keys starting with `OPTIONAL` are excluded from its required-key check.
- `test_production_update_playbook.yml` performs the same process on the
  `staging` inventory group.
- `staging_update_playbook.yml` checks out `main` into `~/esp32-remote-access`
  and operates on its `docker/` directory. It currently uses the default
  `docker-compose.yml`, **not** `docker-compose-staging.yml`.

All three playbooks also upgrade host packages and can reboot the host. They do
not automatically include the local Compose override shown above; reconcile
these deployment limitations in the operator's setup before running them.

## Debugging and packet captures

Enable frontend debug mode in the account page's local settings. It is stored
under `debugMode` in local storage and is independent of the backend's Cargo
build profile. Start a new remote connection after enabling it: worker setup
selects whether to collect packets. The frame exposes **Save Pcap log**; its
Ctrl+Alt+P shortcut also requests a capture. Ctrl+Alt+Shift+R opens the device's
`/recovery` page inside the current tunnel. A direct
`/devices/DEVICE_UUID/recovery` URL uses the same frame route.

The backend has a separate opt-in capture feature. From `backend/`:

```sh
mkdir -p pcap
cargo run --features pcap-logging
```

It writes `pcap/device_DEVICE_UUID.pcapng` relative to the backend working
directory. The directory must already exist and be writable. Captures contain
decrypted IP packets from the **management** tunnel and can include transferred
charge logs. A newly matched tunnel opens/truncates its device's file. This is
different from the browser's capture of traffic inside the remote tunnel; both
can contain device data. Capturing is disabled in normal backend builds.

When troubleshooting:

- Check TLS trust, `/sw.js` activation and browser storage first if login or
  device pages cannot initialize. Both valid cookies and the cached derived
  account key are needed; storage eviction can require another login.
- Check device management connectivity and UDP 51820 if a device appears
  disconnected. `/api/ws` requires an active management tunnel and returns 502
  without one; occupied keys return 409, and no available key returns 404 from
  `/charger/get_key`.
- Check the reported firmware version and matching `*_index.html` file when
  investigating the server's HTML cache. Missing files use the device tunnel;
  supplying an invalid gzip file is not equivalent to a missing file.
- Restart/reconnect after backend changes. Relay maps and reservations are
  process-local; an SQL backup does not preserve active tunnels.

`GET /api/state` is an unauthenticated debug-build state dump, not a production
health endpoint. Keep that distinction when running a development backend.
