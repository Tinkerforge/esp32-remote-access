// z-base-32 from ZRTP (RFC 6189):
//   - Case-insensitive (alphabet itself is lowercase + digits), chosen to
//     avoid visual ambiguities and accidental hostnames.
//   - Notably absent from the alphabet: '0', 'l', 'v', and '2'. The lack of
//     '2' means the z-base-32 encoding of any existing 4-digit UID whose
//     leading decimal digit is '2' is itself a string with no '2' character.
//
// Mirrors the C reference implementation:
//   - Operates on uint32-sized non-negative integers (0 .. 2^32-1).
//   - log32(2^32 - 1) ~= 6.4, so at most 7 chars are ever produced.
const ZBASE32_ALPHABET = 'ybndrfg8ejkmcpqxot1uwisza345h769';

// Maximum length of an encoded uint32 value (excluding any terminator).
export const ZBASE32_MAX_STRLEN = 7;

// Upper bound for uint32 inputs.
const UINT32_MAX = 0xFFFFFFFFn;

export function encodeZBase32(value: bigint): string {
    if (value < 0n || value > UINT32_MAX) {
        throw new Error("encodeZBase32 only supports uint32 values (0 to 2^32-1)");
    }

    // Mirrors the C `do { ... } while (value > 0);` loop. The do-while is
    // what gives us a single-character "y" for an input of 0.
    let result = '';
    let remaining = value;
    do {
        const remainder = remaining % 32n;
        remaining = remaining / 32n;
        result = ZBASE32_ALPHABET[Number(remainder)] + result;
    } while (remaining > 0n);

    return result;
}

export function intToZBase32(input: number): string {
    if (!Number.isInteger(input) || input < 0 || input > 0xFFFFFFFF) {
        throw new Error("intToZBase32 only supports non-negative integers in uint32 range (0 to 2^32-1)");
    }

    return encodeZBase32(BigInt(input));
}
