# Sequence Diagram


### Registration
When beginning no data is stored anywhere.

When ending the backend stores the Encrypted username, key, email, salts and secret.

```mermaid
sequenceDiagram
    Note over Frontend: User enters: <br> - password <br> - username <br> - email

    Frontend->>Backend: Get salt request
    Note over Backend: Generates salt1
    Backend->>Frontend: Sends salt1
    Note right of Backend: Holds: <br> - salt1
    Note over Frontend: Generates salt2. <br> secret-salt = salt1 + salt2 <br> secret-key = argon2(password, secret-salt) <br> Generates-secret <br> encrypted-secret = libsodium.secret_box(secret, secret-key)

    Note left of Frontend: Holds: <br> - username <br> -email <br> - password <br> - unencrypted-secret <br> - encrypted-secret <br> - secret-key <br> - secret-salt

    Frontend->>Backend: Get salt request
    Note over Backend: Generates salt3
    Backend->>Frontend: Sends salt3

    Note right of Backend: Holds: <br> - salt1 <br> - salt3

    Note over Frontend: Generates salt4 <br> login-salt = salt3 + salt4

    Note left of Frontend: Holds: <br> - username <br> -email <br> - password <br> - unencrypted secret <br> - encrypted secret <br> - secret-key <br> - secret-salt <br> - login-key <br> - login-salt

    Frontend->>Backend: Send username, email, encrypted-secret, secret-salt, <br> login-key and login-salt

    Note over Backend: secret-salt.contains(salt1) <br> login-salt.contains(salt3)

    alt all checks pass
        Note right of Backend: Saves <br> - username <br> - email <br> - login-salt <br> - secret-salt <br> - hashed login-key
        Backend->>Frontend: 201 Created
        Note over Frontend: Prompts user to save the unencrypted secret<br>as recovery fallback
    end
    alt one check fails
        Backend->>Frontend: Send Error
    end
```

### Login

When beginning the Backend has stored the encrypted Username, key, email, salts and secret.

When ending no additional data is stored on the backend and the frontend stores a JWT cookie.

```mermaid
sequenceDiagram
    Note over Frontend: User input: <br> - email <br> - password

    Frontend->>Backend: Request login-salt for email
    Backend->>Frontend: Send  login-salt

    Note over Frontend: login-key = argon2(password, login-salt)

    Frontend->>Backend: Send email and login-key
    Note over Backend: Verify login.
    Backend->>Frontend: Respond with JWT and refresh cookie
    Note right of Backend: saves: <br> - refresh token
```

### Refresh Jwt Token
```mermaid
sequenceDiagram
    Note over Frontend, Backend: Login
    Frontend->>Backend: Sends refresh-token
    Note over Backend: validate refresh-token
    alt token is valid
        Note right of Backend: deletes: <br> - old refresh-token <br> saves: <br> new refresh-token
        Backend->>Frontend: Sends new Jwt and refresh-token
    end
    alt token is expired
        Note right of Backend: deletes: <br> - refresh-token
        Backend->> Frontend: Sends Error <br> (Could also send empty jwt and refresh-token)
    end
```
### Logout

```mermaid
sequenceDiagram
    Note over Frontend, Backend: Login
    Frontend->>Backend: Send logout-request
    alt logout current device
        Note right of Backend: deletes: <br> - refresh token
    end
    alt logout all devices
        Note right of Backend: deletes: <br> - all refresh tokens for that user
    end
    Backend->>Frontend: Send empty Jwt and refresh-token
```

### Add charger

When beginning the Backend has stored the encrypted Username, login-key, email, salts and secret.

When ending the backend additionally stores the encrypted wireguard keys and the charger stores the unencrypted secret, login-key and WireGuard keys.

```mermaid
sequenceDiagram
    participant Charger Frontend
    participant Charger
    participant Backend

    Note over Charger Frontend, Backend: Login

    Note left of Charger Frontend: Holds: <br> - login-key
    Charger Frontend->>Backend: Request encrypted-secret

    Backend->>Charger Frontend: Respond with encrypted-secret and secret-salt
    Note over Charger Frontend: secret-key = argon2(password, secret-salt)
    Note over Charger Frontend: Generate WireGuard keys

    Note left of Charger Frontend: Holds: <br> - encrypted secret <br> - secret-key <br> - login-key <br> - WireGuard-keys

    Charger Frontend->>Charger: Send encrypted-secret, secret-key, <br> Wireguard-keys, login-key
    Note over Charger: secret = libsodium.secret_box_open(encrypted-secret, secret-key)
    Note over Charger: encrypted-WireGuard-keys = libsodium.sealed_box(WireGuard-keys, secret)

    Note left of Charger: Saves: <br> - WireGuard keys

    Note over Charger, Backend: Login

    Charger->>Backend: Register charger
    Note over Backend: Generates a login-key for that charger.
    Note over Backend: Generates a wireguard-keypair for management connection
    Note right of Backend: Saves: <br> - encrypted WireGuard keys <br> - hashed charger-login-key <br> - management-keys
    Backend->>Charger: Respond with charger-login-key and <br> management-public-key
```

### Establish management connection

```mermaid
sequenceDiagram
    Charger->>Backend: Sends charger-identify-request
    Note right of Backend: Caches source-ip-addr

    Charger->>Backend: Sends WireGuard-Handshake
    Note over Backend: Bruteforces handshake with all WireGuard management-keys <br> known for source ip
    Note right of Backend: Caches port-ip combination

    Backend->Charger: Management connection established
```

### Port discovery

```mermaid
sequenceDiagram
    Backend->>Charger: Sends Command to open remote-connection <br> containing a random value
    Note left of Backend: Chaches random value

    Charger->>Backend: Sends udp packet from the port that <br> the remote connection is going to use <br> containing the random value
    Note over Backend: Drops random value
    Note left of Backend: Caches the port-ip combination
```

### Connect to charger

When beginning the Backend has stored the encrypted Username, login-key, email, salts and secret.

When ending no additional data is stored on the backend

```mermaid
sequenceDiagram
    Note over Frontend, Backend: Login

    Frontend->>Backend: Request encrypted secret
    Backend->>Frontend: Respond with encrypted-secret and <br> secret-salt

    Note over Frontend: secret-key = argon2(password, secret-salt)
    Note over Frontend: secret = libsodium.secret_box_open(encrypted-secret, secret-key)

    Note left of Frontend: Holds: <br> - secret

    Frontend->>Backend: Request encrypted WireGuard key
    Backend->>Frontend: Response with one of the unused <br> encrypted WireGuard keys

    Note over Frontend: WireGuard-keys = libsodium.sealed_box_open(encrypted-WireGuard-keys, secret)

    Note left of Frontend: Holds: <br> - secret <br> - WireGuard-keys

    Frontend->>Backend: Start Websocket connection
    Backend->Charger: Port discovery
    Frontend->Charger: Establish remote connection
```

### Change password


```mermaid
sequenceDiagram
    Note over Frontend, Backend: Login
    Note over Frontend: User inputs: <br> - password <br> - new-password

    Frontend->>Backend: Request encrypted secret
    Backend->>Frontend: Respond with encrypted-secret and <br> secret-salt

    Note over Frontend: secret-key = argon2(password, secret-salt)
    Note over Frontend: secret = libsodium.secret_box_open(encrypted-secret, secret-key)

    Note left of Frontend: Holds: <br> - secret

    Frontend->>Backend: Request salt
    Note over Backend: Generate salt1
    Note right of Backend: Holds: <br> - salt1
    Backend->>Frontend: Respond with salt1

    Note over Frontend: Generates salt2
    Note over Frontend: new-secret-salt = salt1 + salt2
    Note over Frontend: new-secret-key = argon2(new-password, new-secret-salt)
    Note left of Frontend: Holds: <br> - secret <br> - new-secret-salt <br> - new-secret-key

    Frontend->>Backend: Request login-salt
    Backend->>Frontend: Respond with login-salt
    Note over Frontend: login-key = argon2(password, login-salt)

    Note left of Frontend: Holds: <br> - secret <br> - new-secret-salt <br> - new-secret-key <br> - login-key

    Frontend->>Backend: Request salt
    Note over Backend: Generate salt3
    Note right of Backend: Holds: <br> - salt1 <br> - salt3
    Backend->>Frontend: Respond with salt3

    Note over Frontend: Generates salt4
    Note over Frontend: new-login-salt = salt3 + salt4
    Note over Frontend: new-login-key = argon2(new-password, new-login-salt)

    Note left of Frontend: Holds: <br> - secret <br> - new-secret-key <br> - login-key <br> - new-login-key

    Note over Frontend: encrypted-secret = libsodium.secret_box(secret, secret-key)

    Frontend->>Backend: Send encrypted-secret, new-secret-salt, login-key, <br> new-login-key, new-login-salt

    Note over Backend: new-secret-key.contains(salt1) && <br> new-login-key.contains(salt3)
    Note right of Backend: Update database with new data.
```

### Password recovery

```mermaid
sequenceDiagram
    Frontend->>Backend: Request password recovery
    Note over Backend: Generates a temporary password and <br> sends it via E-Mail.
    Note right of Backend: Holds: <br> - temporary-password

    Note over Frontend: User input: <br> - (maybe) recovery-file, <br> - temporary-password <br> - new-password.

    Note left of Frontend: Holds: <br> - temporary-password <br> - new-password <br> - (maybe) - recovery-password

    Frontend->>Backend: Request salt
    Note over Backend: Generates salt1
    Note right of Backend: Holds: <br> - temporary-password <br> - salt1
    Backend->>Frontend: Respond with salt1

    Note over Frontend: Generates salt2
    Note over Frontend: secret-salt = salt1 + salt2
    Note over Frontend: secret-key = argon2(new-password, secret-salt)

    Note left of Frontend: Holds: <br> - temporary-password <br> - new-password <br> - (maybe) recovery-file <br> - secret-key <br> - secret-salt

    Frontend->>Backend: Request salt
    Note over Backend: Generates salt3.
    Backend->>Frontend: Respont with salt3

    Note right of Backend: Holds: <br> - temporary-password <br> - salt1 <br> - salt3

    Note over Frontend: Generates salt4
    Note over Frontend: login-salt = salt3 + salt4
    Note over Frontend: login-key = argon2(new-password, login-salt)

    Note left of Frontend: Holds: <br> - temporary-password <br> - new-password <br> - (maybe) recovery-password <br> - secret-key <br> - login-key <br> - secret-salt <br> - login-salt

    alt user does not provide recovery file
        Note over Frontend: Generate new-secret
        Note over Frontend: encrypted-new-secret = libsodium.secret_box(new-secret, secret-key)

        Frontend->>Backend: Send login-key, login-salt, secret-salt, <br> encrypted-secret, temporary-password and <br> note that the secret changed

        Note over Backend: login-key.contains(salt3) && <br> secret-key.contains(salt1)
        Note over Backend: Invalidate all keys for the user.
    end

    alt user provides recovery file
        Note over Frontend: Verifies recovery file and extract secret
        Note over Frontend: ecrypted-secret = libsodium.secret_box(secret, secret-key)
        Frontend->>Backend: Send login-key, login-salt, secret-salt, <br> encrypted secret

        Note over Backend: login-key.contains(salt3) && <br> secret-key.contains(salt1)
    end
    Note right of Backend: Saves: <br> - login-key <br> login-salt <br> - secret-salt <br> - encrypted secret
```
