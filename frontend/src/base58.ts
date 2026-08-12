import { intToZBase32 } from './zbase32';

export function encodeBase58Flickr(input: Uint8Array): string {
    const alphabet = '123456789abcdefghijkmnopqrstuvwxyzABCDEFGHJKLMNPQRSTUVWXYZ';
    const bytes = input;

    let value = BigInt(0);
    for (const byte of bytes) {
        value = (value << 8n) + BigInt(byte);
    }

    if (value === 0n) {
        return alphabet[0];
    }

    let result = '';
    while (value > 0n) {
        const remainder = value % 58n;
        value = value / 58n;
        result = alphabet[Number(remainder)] + result;
    }

    // Handle leading zeros
    for (const byte of bytes) {
        if (byte !== 0) break;
        result = alphabet[0] + result;
    }

    return result;
}

// UIDs above this value are rendered as z-base-32 to keep the on-screen
// representation short. The cutoff keeps the historical base58 (Flickr)
// representation for every existing 3-digit UID and every existing 4-digit
// UID whose leading decimal digit is '2' (z-base-32 has no '2', so the
// representation of any future 4+ digit UID can never collide with the
// representation of an existing one).
export const ZBASE32_UID_THRESHOLD = 257899;

// Encode a device UID as a short, human-readable string. UIDs at or below
// `ZBASE32_UID_THRESHOLD` use Flickr-base58 (matching the historical
// representation); larger UIDs fall back to z-base-32 (RFC 6189 / ZRTP)
// so the rendered identifier stays short.
export function encodeUid(uid: number): string {
    if (uid > ZBASE32_UID_THRESHOLD) {
        return intToZBase32(uid);
    }
    return encodeBase58Flickr(numberToBigEndianBytes(uid));
}

// Serialize a non-negative integer as its big-endian byte representation so
// it can be fed to `encodeBase58Flickr` (which expects a Uint8Array).
function numberToBigEndianBytes(value: number): Uint8Array {
    if (!Number.isFinite(value) || value < 0) {
        throw new Error("numberToBigEndianBytes only supports non-negative finite numbers");
    }

    if (value === 0) {
        return new Uint8Array([0]);
    }

    let remaining = BigInt(Math.trunc(value));
    const bytes: number[] = [];
    while (remaining > 0n) {
        bytes.unshift(Number(remaining & 0xffn));
        remaining = remaining >> 8n;
    }

    return new Uint8Array(bytes);
}
