# Data formats and encryption

This describes the formats implemented by the frontend and backend. See
[api.md](api.md) for routes, [sequence.md](sequence.md) for flows, and
[db_connector/src/schema.rs](db_connector/src/schema.rs) for database column types.

## Account keys and salts

The browser derives two separate values from the password using Argon2id with
19 × 1024 KiB of memory (19 MiB), 2 iterations and parallelism 1:

| Value | Output size | Use |
| --- | --- | --- |
| `login_key` | 24 bytes | Sent to the backend as a password-equivalent credential |
| `secret_key` | 32 bytes | Encrypts/decrypts the user's account secret locally |

For registration, password changes and recovery, each salt consists of 24 bytes
from `GET /api/auth/generate_salt` followed by 24 browser-generated random bytes
(48 bytes total). The salt endpoint is stateless: the backend neither caches
issued salts nor verifies that submitted salts contain an issued value. Unknown
email addresses queried through `get_login_salt` receive a cached synthetic salt.

The account `secret` is the 32-byte private key from a libsodium box key pair.
Its public key is derived with `crypto_scalarmult_base(secret)`. The browser
stores the private key on the server encrypted with
`crypto_secretbox_easy(secret, secret_nonce, secret_key)`; the nonce is 24 bytes.
The ciphertext is 48 bytes (32-byte secret plus 16-byte authentication overhead).
The backend stores the ciphertext, nonce and salt, and separately applies
Argon2 to the submitted `login_key` with a fresh server-side salt before storing
its password hash.

The browser keeps the Base64-encoded **derived `secret_key`** in the service
worker's Cache Storage (`secret-cache-v1`, key `secret-key`), and the login salt
in local storage (`loginSalt`). The decrypted account secret and public key are
also held in frontend memory when needed. The service-worker cache survives
worker restarts; it is not merely an in-memory variable. Logout clears the key.

Account display names and email addresses are stored as ordinary database text.
Per-user device names, notes, grouping names and authorization-token names are
sealed to the account public key by the frontend/device; these ciphertexts use
Base64 strings where a JSON/database text field is expected. The backend stores
these fields as opaque strings; it does not enforce their encryption format.

Implementation: [frontend/src/utils.tsx](frontend/src/utils.tsx),
[Register.tsx](frontend/src/components/Register.tsx),
[Login.tsx](frontend/src/components/Login.tsx), [sw.ts](frontend/src/sw.ts).

## Device authorization token

A token copied from the tokens page bundles registration authorization and the
recipient's public key. It is distinct from access/refresh JWTs and from the
UUID identifying a token record in the database.

The complete byte buffer is encoded using the **Flickr Base58** alphabet:

```text
123456789abcdefghijkmnopqrstuvwxyzABCDEFGHJKLMNPQRSTUVWXYZ
```

| Field | Offset | Size | Representation before Base58 encoding |
| --- | --- | --- | --- |
| Authorization credential | 0 | 32 bytes | Random raw token bytes |
| User UUID | 32 | 36 bytes | UTF-8 canonical UUID text, including hyphens |
| User public key | 68 | 32 bytes | Raw libsodium box public key |
| User email | 100 | Variable | UTF-8 bytes; no terminator or length prefix |
| Checksum | Final 32 bytes | 32 bytes | Argon2id digest of all preceding bytes |

The checksum uses an **eight-byte all-zero salt**, 19 × 1024 KiB memory,
2 iterations, parallelism 1 and a 32-byte output. It is an integrity checksum,
not a signature. The email occupies the bytes between the fixed 100-byte prefix
and the final 32-byte digest. Total decoded length is `132 + email_byte_length`.

The database/API represents the raw 32-byte authorization credential with
standard Base64. Devices extract it from the copied Base58 token and send this
Base64 string plus the user UUID to `PUT /api/add_with_token` or use it in
`UserAuth.AuthToken` for `PUT /api/allow_user`.

Token metadata includes `id`, `use_once`, encrypted `name`, `created_at` and
nullable `last_used_at`. API timestamps are Unix seconds. Successful token
validation updates `last_used_at`; a single-use token remains in the database
but is rejected after it has been used. Validation happens before registration
finishes, so a later registration error can still leave it marked used.

`GET /api/auth/service_token` returns `OPTIONAL_SERVICE_AUTH_TOKEN` verbatim
without user authentication. It does not construct, validate or encrypt that
string; an unset or empty value returns HTTP 500.

Implementation: [Tokens.tsx](frontend/src/pages/Tokens.tsx),
[create_authorization_token.rs](backend/src/routes/user/create_authorization_token.rs),
[validate_auth_token](backend/src/utils.rs).

## Recovery file

The downloaded recovery file is JSON containing:

```json
{
  "email": "user@example.com",
  "secret": "<standard Base64 of the raw 32-byte account private key>",
  "hash": "<standard Base64 of SHA-256(UTF-8(email + secret))>"
}
```

The hash covers the exact email and Base64 secret strings concatenated without
a separator. It detects changes to the file; it is not keyed authentication.
The file contains an **unencrypted account private key** and should be kept
private. Recovery still requires the emailed recovery-link token. Reusing this
secret preserves the ability to decrypt existing device keys. Recovering with a
new secret deletes the user's stored WireGuard keys and marks device access
records invalid until devices are configured again.

Implementation: [RecoveryDataComponent.tsx](frontend/src/components/RecoveryDataComponent.tsx),
[Recovery.tsx](frontend/src/pages/Recovery.tsx).

## WireGuard key records

Registration supplies exactly five `Keys` entries per user/device registration:

| Field | Representation |
| --- | --- |
| `web_private` | Sealed-box ciphertext of the Base64 browser private-key string; JSON byte array |
| `psk` | Sealed-box ciphertext of the Base64 preshared-key string; JSON byte array |
| `charger_public` | Standard Base64 device public key |
| `web_address`, `charger_address` | IP network strings with prefix length |
| `connection_no` | Device connection-slot number |

`GET /api/charger/get_key?cid=DEVICE_UUID` returns a matching record with fields
`id`, `charger_id`, `charger_pub`, `charger_address`, `web_private`, `psk` and
`web_address`. The worker opens sealed boxes using the account key pair and
UTF-8-decodes the resulting Base64 strings before passing them to WASM.

Key use is tracked in `AppState.keys_in_use`, not a database `in_use` column.
The WebSocket upgrade reserves a key; disconnect releases it. The key-selection
handler refuses allocation once five keys for that device are active.

The separate **management tunnel** has server-readable key material in
`chargers`: `management_private`, `charger_pub` and `psk` are Base64 strings,
plus the two management IP networks. Device passwords are stored as Argon2
hashes. The browser/device remote tunnel's private key and PSK remain sealed
while relayed through the backend.

## Device identifiers

A device's database/API `id` is a UUID, separate from the numeric hardware `uid`.
Human-readable UIDs use Flickr Base58 up to and including **257899**, and
z-base-32 above that threshold. The z-base-32 alphabet is
`ybndrfg8ejkmcpqxot1uwisza345h769`; the backend accepts either case in that branch.
This is integer encoding, not Base32 encoding of a byte array. The authorization
token format above always uses Flickr Base58 regardless of device UID.

Implementation: [base58.ts](frontend/src/base58.ts),
[zbase32.ts](frontend/src/zbase32.ts),
[charger/add.rs](backend/src/routes/charger/add.rs).
