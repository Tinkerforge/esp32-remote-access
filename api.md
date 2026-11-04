# API reference

The running application mounts routes under **`/api`**. The tables below give
paths relative to that prefix and reflect registration in
[routes/mod.rs](backend/src/routes/mod.rs) and its scope modules.

For request/response schemas, run `cargo run --bin api_docs` from `backend/`.
Swagger UI is at `http://localhost:12345/`; JSON is at
`http://localhost:12345/api-docs/openapi.json`. This separate binary binds to
`0.0.0.0:12345` and serves documentation only. Generated paths omit `/api`.
`frontend/src/schema.d.ts` contains the frontend's generated API types.

## Authentication

JWT-protected routes accept an `access_token` cookie or an
`Authorization: Bearer TOKEN` header. Cookie lookup takes precedence. Refresh
uses the `refresh_token` cookie or a Bearer refresh JWT instead. Login issues
cookies, not a JSON token body. Access JWTs last 6 minutes; refresh JWTs last
60 days and also require a stored session record.

Device endpoints authenticate using credentials in their request bodies;
authorization-token endpoints are distinct from JWT authentication. JSON byte
vectors are arrays of integers; fields explicitly defined as Base64 strings
must remain strings. See [data formats](data_type_definitions.md).

## Account authentication routes

These routes do not require an access JWT.

| Method | Path | Purpose / main input |
| --- | --- | --- |
| GET | `/auth/generate_salt` | Return 24 random bytes; stateless |
| GET | `/auth/get_login_salt?email=...` | Return stored login salt or cached synthetic salt |
| POST | `/auth/register` | Register `name`, `email`, login key/salt and encrypted secret/nonce/salt; returns 201 |
| GET | `/auth/verify?id=...` | Verify an emailed UUID and redirect to the frontend |
| POST | `/auth/resend_verification` | Request another verification email with `{email}` |
| POST | `/auth/login` | Validate `{email, login_key}` and set cookies |
| GET | `/auth/jwt_refresh` | Rotate the refresh session and issue new cookies; requires refresh JWT |
| GET | `/auth/start_recovery?email=...` | Email a recovery link; unknown accounts also return 200 |
| POST | `/auth/recovery` | Consume `recovery_key` and replace login/encrypted-secret fields; `reused_secret` controls device-key invalidation |
| GET | `/auth/service_token` | Return `OPTIONAL_SERVICE_AUTH_TOKEN` verbatim, without authentication; unset/empty returns 500 |

Email-related requests can send `X-Lang`, for example `de` or `en`.

## User routes

All require an access JWT.

| Method | Path | Purpose / main input |
| --- | --- | --- |
| GET | `/user/me` | Current account `id`, `name`, `email`, `has_old_charger` |
| GET | `/user/get_secret` | Encrypted account `secret`, `secret_nonce`, `secret_salt` |
| PUT | `/user/update_user` | Update `{name, email}`; email changes start verification |
| PUT | `/user/update_password` | Verify `old_login_key`, store new login/encrypted-secret fields |
| GET | `/user/logout?logout_all=false` | Delete current refresh session and clear cookies; `true` deletes all refresh sessions |
| DELETE | `/user/delete` | Delete account, requiring `{login_key}` |
| POST | `/user/create_authorization_token` | Create token with `{use_once, name}`; returns 201 and token metadata |
| GET | `/user/get_authorization_tokens` | Return `{tokens: [...]}` including raw Base64 credentials and metadata |
| DELETE | `/user/delete_authorization_token` | Delete the user's token using `{id}` |

The frontend seals authorization-token names to the account public key. The
backend handles them as opaque strings.

## Device and relay routes

| Method | Path | Authentication | Purpose |
| --- | --- | --- | --- |
| PUT | `/charger/add` | Access JWT | Register/update a device with five remote-key records |
| DELETE | `/charger/remove` | Access JWT | Remove current user's association using `{charger: UUID}`; delete device when last user leaves |
| GET (WebSocket) | `/charger/get_devices` | Access JWT | Initial device list and live state changes |
| POST | `/charger/info` | Access JWT | `{charger: UUID}` → port, connection status, firmware, MTU and name |
| POST | `/charger/update_note` | Access JWT | Update sealed `{charger_id, note}` for current user |
| GET | `/charger/get_key?cid=...` | Access JWT | Select a remote key record; 404 when no key is available |
| GET (WebSocket) | `/ws?key_id=...` | Access JWT + key ownership | Relay encrypted WireGuard datagrams |
| GET | `/webinterface?charger=...` | Access JWT + device access | Serve firmware-version-specific HTML file |
| PUT | `/add_with_token` | Authorization token + user UUID | Register/update without login cookies |
| PUT | `/allow_user` | Device credentials + target user's `LoginKey` or `AuthToken` | Store target user's access and five remote keys |
| PUT | `/charger/allow_user` | Access JWT plus the same body credentials | Legacy alias of `/allow_user` |
| PUT | `/management` | Device credentials | Identify/reconcile device and report metadata |
| DELETE | `/selfdestruct` | Device credentials | Delete device and all its user associations/keys |
| POST | `/send_chargelog_to_user` | Device credentials in multipart metadata | Email uploaded charge log |

`/selfdestruct` is registered at the API root: use `/api/selfdestruct`.
The generated document includes `/ws`; the JWT-protected legacy
`/charger/allow_user` alias shares the body schema documented for `/allow_user`.

### Device list WebSocket

Connect to `wss://HOST/api/charger/get_devices` with the access cookie (or a
Bearer header when the client supports custom upgrade headers). The first
**text frame** contains a JSON array. Later text frames contain:

```json
{
  "type": "state_change",
  "charger": {
    "id": "550e8400-e29b-41d4-a716-446655440000",
    "uid": 12345,
    "name": "<Base64 sealed name>",
    "note": null,
    "status": "Connected",
    "port": 80,
    "valid": true,
    "last_state_change": 1780000000,
    "firmware_version": "2.6.7",
    "added_at": null
  }
}
```

`status` serializes as `"Connected"` or `"Disconnected"`, not an integer.
Timestamps are Unix seconds or `null`; old associations may have no `added_at`.
The backend currently tracks one notification session per user UUID. This is
not a plain HTTP GET returning JSON; clients expecting the old response must
use a WebSocket upgrade and read its initial text frame.

### Remote relay WebSocket

Fetch a key with `/charger/get_key?cid=DEVICE_UUID`, decrypt it locally, then
upgrade `/ws?key_id=KEY_UUID`. A successful upgrade reserves the key in memory.
Each **binary frame** carries an encrypted WireGuard packet; no JSON envelope
is used. A live device management tunnel is required. Disconnect releases the
key and sends a device `Disconnect` command.

Both WebSocket handlers send ping frames every 5 seconds and close the
connection after 15 seconds without a ping/pong heartbeat. Protocol clients must
handle ping/pong frames; browser WebSocket implementations do this themselves.

### Management V2

The JSON envelope is externally tagged by data version:

```json
{
  "data": {
    "V2": {
      "id": "550e8400-e29b-41d4-a716-446655440000",
      "password": "<device password from registration>",
      "port": 80,
      "firmware_version": "2.6.7",
      "configured_users": [
        {"user_id": "550e8400-e29b-41d4-a716-446655440001", "name": "<sealed name>"}
      ],
      "mtu": 1240
    }
  }
}
```

`configured_users` can identify a user by email instead of UUID. The response
contains `time`, parallel arrays `configured_users`, `configured_users_emails`
and `configured_users_uuids`, plus nullable `uuid` for legacy device migration.
Older V1 data and top-level numeric `id`/`password` remain supported. The device
type is inferred from the `User-Agent` header. Missing MTU is retained in the
database; `/charger/info` falls back to 1240 when none is stored.

### Charge logs

`POST /send_chargelog_to_user` accepts multipart fields `json` (metadata) and
`chargelog` (file). Metadata includes `charger_uuid`, device `password`,
`user_uuid`, `filename`, `display_name`, `monthly_send` and optional `lang`.
The HTTP handler currently selects language from `X-Lang`. The backend also
supports charge-log request/metadata packets and TCP transfer inside the
management tunnel; see [packet.rs](backend/src/udp_server/packet.rs) and
[multiplex.rs](backend/src/udp_server/multiplex.rs) for the binary protocol.
Those logs are processed by the backend for email delivery.

## Grouping routes

All require an access JWT; updates verify ownership of the grouping. Adding a
device additionally verifies the user's device access. Group membership does
not grant access.

| Method | Path | Main input / response |
| --- | --- | --- |
| GET | `/grouping/list` | `{groupings: [{id, name, device_ids, is_default}]}` |
| POST | `/grouping/create` | `{name, is_default}` → created grouping; `is_default` defaults to false |
| PUT | `/grouping/edit` | `{grouping_id, name?, is_default?}` → updated grouping |
| DELETE | `/grouping/delete` | `{grouping_id}` |
| POST | `/grouping/add_device` | `{grouping_id, device_id}` → membership |
| DELETE | `/grouping/remove_device` | `{grouping_id, device_id}` |

The frontend sends grouping names as Base64 sealed-box ciphertext. Setting a
new default clears the previous default, and a database partial unique index
enforces at most one default per user. Membership pairs are unique.

## Other routes and error behavior

`POST /api/check_expiration` takes `{token_type: "Recovery" | "Verification",
token: UUID}` and returns a JSON boolean indicating whether the token is valid.
Verification tokens must exist and be unexpired; expired registration rows can
remain stored during account retention. Recovery tokens are checked for row
existence, with expiry enforced by periodic cleanup. The endpoint is IP-rate-limited.

`GET /api/state` is registered only with `debug_assertions`; it exposes in-memory
relay state without JWT protection. It is absent in release builds.

Errors are often plain text, rather than a common JSON error object. Check
[error.rs](backend/src/error.rs) and each handler for exact status codes. Missing
accounts/devices/groupings commonly return 400 rather than 404; unavailable keys
return 404, and invalid credentials/access return 401. The backend has no
dedicated health-check route.

### Rate limits

In normal builds, the shared login limiter allows 5 requests/second with burst
25 per `(email, source IP)` across login, login-salt lookup, recovery start and
verification resend. The device HTTP limiter allows 5 requests/minute with burst
25 per `(device identifier, source IP)` for its guarded routes, including
management, user authorization, self-unregistration and charge-log upload.
`/check_expiration` uses 5 requests/second with burst 25 per source IP. These are
limits on the handlers that explicitly check a limiter, not a global middleware
limit on every route.

Rejected HTTP requests return 429, a plain-text wait message, and `retry-after`
and `x-retry-after` headers in seconds. Tests lower the shared rates to 1 and the
burst to 5. UDP global key searches have separate limits: 20/minute per IP and
2/second aggregate with burst 4. See [rate_limit.rs](backend/src/rate_limit.rs).
