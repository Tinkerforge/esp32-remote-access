#!/usr/bin/env python3

"""Sign a string using libsodium's combined mode and base64-encode the result.

This uses libsodium's ``crypto_sign`` function, which signs the message and
produces ``signature || message`` in a single call. The output is then
base64-encoded so it can be shipped as a string (e.g. embedded in a JSON
payload or a header).

Unlike the streaming ``crypto_sign_init`` / ``crypto_sign_update`` /
``crypto_sign_final_create`` API (Ed25519ph), the combined mode uses plain
Ed25519 (the message is signed directly, not prehashed). Verification should
use libsodium's ``crypto_sign_open`` from the same family. A pure Ed25519
verifier (e.g. Python's ``cryptography`` library) can verify the extracted
64-byte signature against the original message.

Usage:
    # Sign a string with a hex-encoded Ed25519 secret key
    ./sign_string.py --secret-key <hex> "the string to sign"

    # Or pipe the string on stdin
    echo -n "the string to sign" | ./sign_string.py --secret-key <hex>

    # Read the message from a file
    ./sign_string.py --secret-key <hex> --input-file firmware.bin > sig.b64

    # Generate an Ed25519 keypair (libsodium format: 64-byte sk = seed || pk)
    ./sign_string.py --generate-keypair

    # Read the secret key from a KeePassXC database (mirrors the upstream
    # Tinkerforge/esp32-firmware sign.py behavior). In password mode the
    # database password is prompted for at runtime and not echoed to the
    # terminal.
    ./sign_string.py --keepass path/to/store.kdbx "the string to sign"
    ./sign_string.py --keepass store.kdbx --keepass-protection keyfile \\
        --keepass-keyfile store.keyfile "msg"
    ./sign_string.py --keepass store.kdbx --keepass-protection token \\
        --keepass-token 12345678 "msg"

A libsodium Ed25519 secret key is 64 bytes (the seed concatenated with the
public key). Generate it via libsodium's crypto_sign_keypair or via the
``--generate-keypair`` flag below. The secret key can also be the 32-byte
seed alone; libsodium accepts both.

When using ``--keepass``, the secret key is the ``password`` attribute of the
configured entry (default entry name: ``sodium_secret_key``). This matches
the upstream sign.py which reads ``sodium_secret_key`` from
``sodium_secret_key_path`` (a ``.kdbx`` file) protected by a password, a
keyfile, or a YubiKey/Nitrokey challenge-response token (slot 2).
"""

import argparse
import base64
import ctypes
import ctypes.util
import getpass
import os
import subprocess
import sys


def load_libsodium():
    libsodium_path = ctypes.util.find_library("sodium")
    if libsodium_path is None:
        raise RuntimeError(
            "Cannot find libsodium. Install libsodium (e.g. `apt install libsodium23`)."
        )

    libsodium = ctypes.cdll.LoadLibrary(libsodium_path)

    if libsodium.sodium_init() < 0:
        raise RuntimeError("libsodium sodium_init failed")

    libsodium.crypto_sign_publickeybytes.restype = ctypes.c_size_t
    libsodium.crypto_sign_secretkeybytes.restype = ctypes.c_size_t
    libsodium.crypto_sign_bytes.restype = ctypes.c_size_t

    return libsodium


def generate_keypair(libsodium):
    pk_bytes = libsodium.crypto_sign_publickeybytes()
    sk_bytes = libsodium.crypto_sign_secretkeybytes()

    pk_buf = ctypes.create_string_buffer(pk_bytes)
    sk_buf = ctypes.create_string_buffer(sk_bytes)

    if libsodium.crypto_sign_keypair(pk_buf, sk_buf) < 0:
        raise RuntimeError("libsodium crypto_sign_keypair failed")

    return bytes(pk_buf), bytes(sk_buf)


def sign_string(message: bytes, secret_key: bytes) -> bytes:
    """Sign ``message`` with the given Ed25519 ``secret_key`` using libsodium's
    combined mode (``crypto_sign``).

    The combined mode produces ``signature || message`` in a single call. This
    is plain Ed25519 (the message is signed directly, not prehashed), so the
    leading 64 bytes of the returned blob can be verified against the
    trailing message by any standard Ed25519 verifier. Use libsodium's
    ``crypto_sign_open`` to verify the combined blob as-is.
    """

    libsodium = load_libsodium()

    sk_bytes = libsodium.crypto_sign_secretkeybytes()
    sig_bytes = libsodium.crypto_sign_bytes()

    if len(secret_key) != sk_bytes:
        raise ValueError(
            f"secret key must be {sk_bytes} bytes (got {len(secret_key)})"
        )

    # Combined mode: output is signature || message, total size = sig_bytes + len(message)
    out_buf = ctypes.create_string_buffer(sig_bytes + len(message))
    out_len = ctypes.c_ulonglong(0)

    if libsodium.crypto_sign(
        out_buf,
        ctypes.byref(out_len),
        message,
        len(message),
        secret_key,
    ) < 0:
        raise RuntimeError("libsodium crypto_sign failed")

    return bytes(out_buf[:out_len.value])


def keepassxc_get_secret_key(
    db_path,
    entry,
    protection,
    *,
    password=None,
    keyfile=None,
    token=None,
):
    """Read the ``password`` attribute of ``entry`` from a KeePassXC database
    using ``keepassxc-cli``. Mirrors the ``keepassxc()`` helper from the
    upstream Tinkerforge/esp32-firmware sign.py.

    ``protection`` is one of ``"password"``, ``"keyfile"`` or ``"token"``.
    Returns the password string with whitespace stripped, or ``None`` if the
    entry could not be read.
    """
    args = ["keepassxc-cli", "show"]
    kwargs = {"encoding": "utf-8"}
    stdin_input = None

    if protection == "token":
        if token is None:
            raise ValueError("token protection requires --keepass-token")
        args += ["--no-password", "-y", f"2:{token}"]
    elif protection == "keyfile":
        if keyfile is None:
            raise ValueError("keyfile protection requires --keepass-keyfile")
        args += ["-q", "--no-password", "-k", keyfile]
        kwargs["stderr"] = subprocess.DEVNULL
    elif protection == "password":
        kwargs["stderr"] = subprocess.DEVNULL
        if password is None:
            raise ValueError("password protection requires --keepass-password")
        stdin_input = password + "\n"
    else:
        raise ValueError(
            f"invalid protection: {protection!r} (expected password/keyfile/token)"
        )

    args += ["-s", "-a", "password", db_path, entry]

    if stdin_input is not None:
        kwargs["input"] = stdin_input

    try:
        out = subprocess.check_output(args, **kwargs)
    except Exception:
        return None

    return out.strip()


def main():
    parser = argparse.ArgumentParser(
        description="Sign a string with Ed25519 (libsodium) and base64-encode it, "
                    "matching the Tinkerforge/esp32-firmware sign.py behavior."
    )
    parser.add_argument(
        "string",
        nargs="?",
        help="String to sign. If omitted, the message is read from stdin.",
    )
    parser.add_argument(
        "--secret-key",
        help="Hex-encoded Ed25519 secret key (64 bytes / 128 hex chars).",
    )
    parser.add_argument(
        "--secret-key-file",
        help="Path to a file containing the hex-encoded Ed25519 secret key.",
    )
    parser.add_argument(
        "--keepass",
        metavar="KDBX",
        help="Path to a KeePassXC database (.kdbx) holding the secret key, "
             "matching the upstream sign.py. The secret key is read from the "
             "``password`` attribute of the configured entry.",
    )
    parser.add_argument(
        "--keepass-entry",
        default="sodium_secret_key",
        help="KeePass entry name to read the secret key from (default: "
             "%(default)s; matches the upstream ``sodium_secret_key`` entry).",
    )
    parser.add_argument(
        "--keepass-protection",
        choices=("password", "keyfile", "token"),
        default="password",
        help="How the KeePass database is unlocked (default: %(default)s). "
             "``token`` uses a YubiKey/Nitrokey on slot 2.",
    )
    parser.add_argument(
        "--keepass-keyfile",
        help="Path to the keyfile used with ``--keepass-protection keyfile``.",
    )
    parser.add_argument(
        "--keepass-token",
        help="YubiKey/Nitrokey token secret for ``--keepass-protection token``.",
    )
    parser.add_argument(
        "--input-file",
        help="Read the message from this file instead of stdin / positional arg.",
    )
    parser.add_argument(
        "--generate-keypair",
        action="store_true",
        help="Print a freshly generated Ed25519 keypair (hex) and exit.",
    )

    args = parser.parse_args()

    if args.generate_keypair:
        libsodium = load_libsodium()
        pk, sk = generate_keypair(libsodium)
        print(f"sodium_public_key: {pk.hex()}")
        print(f"sodium_secret_key: {sk.hex()}")
        return 0

    # Read the message
    if args.input_file is not None:
        if args.string is not None:
            parser.error("cannot pass both a positional string and --input-file")
        with open(args.input_file, "rb") as f:
            message = f.read()
    elif args.string is not None:
        message = args.string.encode("utf-8")
    else:
        message = sys.stdin.buffer.read()

    # Read the secret key
    secret_key = None

    if args.keepass is not None:
        keepass_password = None
        if args.keepass_protection == "password":
            # Prefer the env var for non-interactive/CI use; otherwise prompt
            # interactively so the password is never echoed and never appears
            # on the command line.
            if os.environ.get("KEEPASS_PASSWORD") is not None:
                keepass_password = os.environ["KEEPASS_PASSWORD"]
            else:
                try:
                    keepass_password = getpass.getpass(
                        prompt=f"Enter password for KeePass database {args.keepass}: "
                    )
                except KeyboardInterrupt:
                    parser.error("aborted while prompting for KeePass password")

        secret_key_hex = keepassxc_get_secret_key(
            args.keepass,
            args.keepass_entry,
            args.keepass_protection,
            password=keepass_password,
            keyfile=args.keepass_keyfile,
            token=args.keepass_token,
        )

        if secret_key_hex is None:
            parser.error(
                f"could not read entry {args.keepass_entry!r} from "
                f"{args.keepass} (protection={args.keepass_protection})"
            )

        try:
            secret_key = bytes.fromhex(secret_key_hex)
        except ValueError as e:
            parser.error(f"keepass entry {args.keepass_entry!r} is not valid hex: {e}")
    else:
        secret_key_hex = None
        if args.secret_key is not None and args.secret_key_file is not None:
            parser.error("pass only one of --secret-key / --secret-key-file")

        if args.secret_key is not None:
            secret_key_hex = args.secret_key
        elif args.secret_key_file is not None:
            with open(args.secret_key_file, "r", encoding="utf-8") as f:
                secret_key_hex = f.read().strip()
        elif os.environ.get("ED25519_SECRET_KEY") is not None:
            secret_key_hex = os.environ["ED25519_SECRET_KEY"].strip()
        else:
            parser.error(
                "no secret key provided: use --secret-key, --secret-key-file, "
                "--keepass, ED25519_SECRET_KEY env var, or --generate-keypair"
            )

        try:
            secret_key = bytes.fromhex(secret_key_hex)
        except ValueError as e:
            parser.error(f"secret key is not valid hex: {e}")

    signature = sign_string(message, secret_key)
    print(len(signature))
    encoded = base64.b64encode(signature).decode("ascii")

    if os.environ.get("SIGN_STRING_NEWLINE") == "1":
        print(encoded)
    else:
        sys.stdout.write(encoded)

    return 0


if __name__ == "__main__":
    try:
        sys.exit(main())
    except Exception as e:  # pragma: no cover - CLI error reporting
        print(f"Error: {e}", file=sys.stderr)
        sys.exit(1)
