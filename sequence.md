# Sequence Diagram


### Registration
When beginning no data is stored anywhere.

When ending the backend stores the Encrypted username, key, email, salts and secret.

```mermaid
sequenceDiagram
    Frontend->>Backend: Get salt request
    Backend->>Frontend: Sends newly generated salt
    Note over Frontend: Also generates salt,<br>concatinates both salts,<br>derives a key with it and the password,<br>generates a secret and<br>encrypts the generated secret using the derived.
    Note left of Frontend: Holds: <br> - unencrypted secret <br> - encrypted secret <br> - secret-key
    Frontend->>Backend: Get salt request
    Backend->>Frontend: Sends newly generated salt
    Note over Frontend: Also generates a second salt,<br>concatinates both salts and<br> derives a key from it and the password.
    Note left of Frontend: Holds: <br> - unencrypted secret <br> - encrypted secret <br> - secret-key <br> - login-key
    Frontend->>Backend: Send registration schema containing the encrypted secret,<br> login-key and both concatinated salts
    Note over Backend: Encrypts the username, email and salts and <br> hashes the login-key.
    Note right of Backend: Saves <br> - encrypted username <br> - encrypted email <br> - encrypted login-salt <br> - encrypted secret-salt <br> - hashed login-key
    Backend->>Frontend: 201 Created
    Note over Frontend: Prompts user to save the unencrypted first secret<br>as recovery fallback
```

### Login

When beginning the Backend has stored the encrypted Username, key, email, salts and secret.

When ending no additional data is stored on the backend and the frontend stores a JWT cookie.

```mermaid
sequenceDiagram
    Frontend->>Backend: Request salt for login-key
    Note over Backend: Encrypts username from request,<br>searches for encrypted username in database and<br>decrypts key-salt.
    Backend->>Frontend: Send decrypted login-key-salt
    Note over Frontend: Derives login-key with salt.
    Frontend->>Backend: Send login with login-key
    Note over Backend: Verify login.
    Backend->>Frontend: Respond with JWT cookie
```

### Add charger

When beginning the Backend has stored the encrypted Username, login-key, email, salts and secret.

When ending the backend additionally stores the encrypted wireguard keys and the charger stores the unencrypted secret, login-key and WireGuard keys.

```mermaid
sequenceDiagram
    participant Charger Frontend
    participant Charger
    participant Backend

    Charger Frontend->>Backend: Request login-key-salt
    Backend->>Charger Frontend: Respond with login-key-salt
    Note over Charger Frontend: Derive login-key with user password and salt.
    Note left of Charger Frontend: Holds: <br> - login-key
    Charger Frontend->>Backend: Request encrypted secret
    Backend->>Charger Frontend: Respond with encrypted secret and salt for it
    Note over Charger Frontend: Derive key with user password and salt and<br>decrypt secret with it.
    Note over Charger Frontend: Generate WireGuard keys
    Note left of Charger Frontend: Holds: <br> - decrypted secret <br> - secret-key <br> - login-key <br> - unencrypted WireGuard keys
    Charger Frontend->>Charger: Save configuration containing<br>unencrypted secret, login key<br>and WireGuard keys
    Note over Charger: Encrypt WireGuard keys used for<br>remote connections using the secret.
    Note left of Charger: Saves: <br> - unencrypted secret <br> - login-key <br> - unencrypted WireGuard keys
    Charger->>Backend: Request Login
    Note over Backend: Verify login
    Backend->>Charger: Respond with JWT cookie
    Charger->>Backend: Send encrypted WireGuard keys
    Note right of Backend: Saves: <br> - encrypted WireGuard keys
```

### Connect to charger

When beginning the Backend has stored the encrypted Username, login-key, email, salts and secret.

When ending no additional data is stored on the backend

```mermaid
sequenceDiagram
    Frontend->>Backend: Request encrypted secret
    Backend->>Frontend: Respond with encrypted secret and <br> salt for it
    Note over Frontend: Derive key from user password and <br> salt and decrypt secret.
    Note left of Frontend: Holds: <br> - decrypted secret
    Frontend->>Backend: Request encrypted WireGuard key
    Backend->>Frontend: Response with one of the unused <br> encrypted WireGuard keys
    Note over Frontend: Decrypt WireGuard key using the secret
    Note left of Frontend: Holds: <br> - decrypted secret <br> - decrypted WireGuard key
    Frontend->>Backend: Start Websocket connection
    Backend->>Charger: Send command to open <br> remote connection
    Charger->>Backend: Send port discovery
```

### Change password


```mermaid
sequenceDiagram
    Frontend->>Backend: Request encrypted secret
    Backend->>Frontend: Respond with encrypted secret and <br> salt for it
    Note over Frontend: Derive secret-key from user password and <br> salt and decrypt secret.
    Note left of Frontend: Holds: <br> - decrypted secret
    Frontend->>Backend: Request salt
    Note over Backend: Generate a new salt
    Backend->>Frontend: Respond with salt
    Note over Frontend: Generate a second salt concatinate <br> both and derive a new secret-key from <br> the user password and salt.
    Note left of Frontend: Holds: <br> - decrypted secret <br> - new secret-key
    Frontend->>Backend: Request login-key-salt
    Backend->>Frontend: Respond with login-key-salt
    Note over Frontend: Derive login-key from user password and <br> login-key-salt.
    Note left of Frontend: Holds: <br> - decrypted secret <br> - new secret key <br> - current login-key
    Frontend->>Backend: Request salt
    Note over Backend: Generate a new salt
    Backend->>Frontend: Respond with salt
    Note over Frontend: Generate a second salt concatinate <br> both and derive a new login-key from <br> the user password and salt.
    Note left of Frontend: Holds: <br> - decrypted secret <br> - new secret key <br> - current login-key <br> - new login-key
    Note over Frontend: Encrypt secret with new secret-key.
    Frontend->>Backend: Send encrypted secret, <br> old login-key and new login-key
    Note over Backend: Update database with new data.
```

### Password recovery

```mermaid
sequenceDiagram
    Frontend->>Backend: Request password recovery
    Note over Backend: Generates a temporary password and <br> sends it via E-Mail.
    Note right of Backend: Holds: <br> - temporary password
    Note over Frontend: User is prompted for recovery file, <br> temporary password and new password.
    Note left of Frontend: Holds: <br> - temporary password <br> - new password <br> - (probably) recovery password
    Frontend->>Backend: Request salt
    Note over Backend: Generates new salt.
    Backend->>Frontend: Respond with salt
    Note over Frontend: Also generates a salt, concatinates both salts and <br> derives a new secret key from user password and salt.
    Note left of Frontend: Holds: <br> - temporary password <br> - new password <br> - (probably) recovery password <br> - secret-key <br> - secret-key-salt
    Frontend->>Backend: Request salt
    Note over Backend: Generates new salt.
    Backend->>Frontend: Respont with salt
    Note over Frontend: Also generates a salt, concatinates both salts and <br> derives a new login key from user password and salt.
    Note left of Frontend: Holds: <br> - temporary password <br> - new password <br> - (probably) recovery password <br> - secret-key <br> - login-key <br> - secret-key-salt <br> - login-key-salt
    alt user does not provide recovery file
        Note over Frontend: Generate a new secret and encrypt it with <br> the secret-key.
        Frontend->>Backend: Send login-key, secret-key-salt, <br> encrypted secret and note that <br> the secret changed
        Note over Backend: Invalidate all keys for the user.
    end
    alt user provides recovery file
        Note over Frontend: Verifies recovery file and encrypt secret <br> contained in it.
        Frontend->>Backend: Send login-key, secret-key-salt, <br> encrypted secret
    end
    Note over Backend: Saves: <br> - login-key <br> - secret-key-salt <br> - encrypted secret
```
