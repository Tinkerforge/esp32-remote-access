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
