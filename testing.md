# Testing and development checks

Run commands from the indicated crate/package directory. There is no root
Cargo workspace. See [README.rst](README.rst) for build dependencies and local
TLS setup.

## Backend and database

Use a **disposable PostgreSQL database**. Tests create and delete users/devices;
the `db_connector` migration test deletes table contents and reverts/reapplies
migrations. Do not point these tests at a development database containing data
you need to keep, or at a production database.

CI and Compose both use PostgreSQL 16. Set the environment
for your test database, for example:

```sh
export DATABASE_URL=postgres://postgres:testing@localhost:5432/remote_access_test
export JWT_SECRET=testing123
export FRONTEND_URL=https://localhost:8081
```

Create that database first. Unlike normal backend startup, the test harness
expects migrations to have been applied. With `diesel_cli` installed:

```sh
cargo install diesel_cli --no-default-features --features postgres
cd db_connector
diesel migration run
cargo test
cd ../backend
cargo test
cargo test --release --lib state_endpoint_is_debug_only -- --nocapture
cargo fmt --check
cargo clippy -- -D warnings
```

The release test verifies that `/api/state`, which dumps relay state, is not
registered in production builds. Tests use application test state and suppress
real SMTP delivery. They still require the database and the environment above.
Run backend and database test commands sequentially; the migration test changes
the shared schema.

[Backend CI](.github/workflows/backend-tests.yml) runs migrations, debug backend
tests, the release state-endpoint test, database tests, formatting and Clippy.

## Frontend unit tests

Build the local `wg-webclient/pkg` dependency before the initial `npm ci`:

```sh
cd wg-webclient
wasm-pack build --target bundler
cd ../frontend
npm ci
npm run lint
npm run typecheck
npm run test:unit:run
```

`lint` runs **Oxlint**. `typecheck` first rebuilds WASM with `--target web`, then
runs `tsc --noEmit`. Before building/running the actual frontend afterward,
rebuild `wg-webclient` with `wasm-pack build --target bundler`: the worker uses
static imports and does not initialize the `web` target's WASM module explicitly.
`test:unit:run` uses Vitest/jsdom and the mocks in `src/test-setup.ts`; it does not
need the running backend, Mailisk or a device. Use `npm run test:unit` for watch
mode. Unit-test aliases select WARP branding.

[Frontend CI](.github/workflows/frontend-tests.yml) uses Node 24, stable Rust and
wasm-pack, and runs lint, type checking and unit tests. Its initial `npm ci`
precedes the WASM build; if a clean checkout cannot resolve the local package,
build WASM first as shown here.

## Browser integration tests

Playwright tests operate on an **already running** service. The config does not
start a web server. Copy `frontend/.env.local.example` to `.env.local`, then set:

- `TEST_DOMAIN` to the full frontend URL, **including** the HTTPS port when
  applicable, for example `https://host:8081`. `TEST_PORT` only overrides the
  port configured on the charger; it is not appended to browser navigation URLs.
- `TEST_USER_NAME1/2` and `TEST_PASSWORD1/2` for the two test accounts.
- `MAILISK_API_KEY`, `MAILISK_NAMESPACE` and `MAILISK_USER1/2` for account/email
  flows. The email addresses are `USER@NAMESPACE.mailisk.net`; the full lifecycle
  suite needs these values even when hardware tests are skipped.
- `TEST_WALLBOX_DOMAIN` and `TEST_WALLBOX_UID` for hardware tests. Tests with
  explicit hardware guards skip when these are missing.
- `NEED_CUSTOM_CERT=true` when the device must select its uploaded custom CA,
  and `NODE_EXTRA_CA_CERTS` where needed for Node-side requests. Browser and
  device TLS trust must also be configured.

From `frontend/`:

```sh
npx playwright install chromium
npm test
```

The npm lifecycle runs registration/verification from `pre-test/`, the Chromium
suite from `tests/`, then account deletion from `post-test/`. These tests change
account settings, configure the device's relay, register/remove devices, test
remote recovery and delete the test accounts. Use dedicated accounts and test
hardware. The main suite runs with one worker.

To run a selected test without npm's registration/deletion lifecycle:

```sh
npx playwright test tests/00-relay-settings.spec.ts --project=chromium
```

Other authenticated tests require accounts to be prepared first. Firefox and
WebKit projects are declared in the configs, but the npm scripts select Chromium.

## WASM tests

`wg-webclient` targets `wasm32-unknown-unknown` through `.cargo/config.toml`.
Its tests use `wasm-bindgen-test`, including worker-based network-stack tests.
Run them from that crate with a browser runner, for example:

```sh
wasm-pack test --headless --chrome
```

This is separate from the frontend Vitest and Playwright suites.

## Regenerate API declarations

The backend's Utoipa annotations generate the OpenAPI document; the frontend's
`src/schema.d.ts` is a snapshot of that document. After changing documentation
annotations or request/response schemas, start the documentation server from
`backend/`:

```sh
cargo run --bin api_docs
```

While it is running, regenerate from `frontend/` (7.13.0 is the generator version
used for the current snapshot):

```sh
npx openapi-typescript@7.13.0 http://localhost:12345/api-docs/openapi.json -o src/schema.d.ts
./node_modules/.bin/tsc --noEmit
```

Stop the documentation server afterward. Type checking directly here avoids
the `npm run typecheck` script's WASM rebuild and target change. The document
omits the runtime `/api` prefix; the frontend adds it through `fetchClient`.
