import { describe, it, expect } from 'vitest';
import { intToZBase32, encodeZBase32, ZBASE32_MAX_STRLEN } from './zbase32';

// Alphabet used by encodeZBase32 (z-base-32 from ZRTP / RFC 6189):
// 'ybndrfg8ejkmcpqxot1uwisza345h769'
//
//   index |  0  1  2  3  4  5  6  7  8  9 10 11 12 13 14 15
//   char  |  y  b  n  d  r  f  g  8  e  j  k  m  c  p  q  x
//
//   index | 16 17 18 19 20 21 22 23 24 25 26 27 28 29 30 31
//   char  |  o  t  1  u  w  i  s  z  a  3  4  5  h  7  6  9

describe('encodeZBase32', () => {
  it('encodes 0 as "y"', () => {
    expect(encodeZBase32(0n)).toBe('y');
  });

  it('encodes 1 as "b"', () => {
    expect(encodeZBase32(1n)).toBe('b');
  });

  it('encodes 2 as "n"', () => {
    expect(encodeZBase32(2n)).toBe('n');
  });

  it('encodes 31 as "9"', () => {
    // alphabet[31] === '9'
    expect(encodeZBase32(31n)).toBe('9');
  });

  it('encodes 32 as "by"', () => {
    // 32 = 1*32 + 0 -> indices [1, 0] -> 'b' + 'y'
    expect(encodeZBase32(32n)).toBe('by');
  });

  it('encodes 1024 as "byy"', () => {
    // 1024 = 1*32^2 + 0*32 + 0 -> indices [1, 0, 0] -> 'byy'
    expect(encodeZBase32(1024n)).toBe('byy');
  });

  it('encodes 32^3 - 1 (32767) as "999"', () => {
    // 32767 = 31*32^2 + 31*32 + 31 -> indices [31, 31, 31] -> '999'
    expect(encodeZBase32(32767n)).toBe('999');
  });

  it('encodes the threshold value 257899 as "855m"', () => {
    // 257899 = 7*32^3 + 27*32^2 + 27*32 + 11 -> indices [7, 27, 27, 11] -> '855m'
    expect(encodeZBase32(257899n)).toBe('855m');
  });

  it('encodes 257900 as "855c"', () => {
    // 257900 = 7*32^3 + 27*32^2 + 27*32 + 12 -> indices [7, 27, 27, 12] -> '855c'
    expect(encodeZBase32(257900n)).toBe('855c');
  });

  it('encodes UINT32_MAX as "d999999" (7 chars max)', () => {
    // 0xFFFFFFFF = 2^32 - 1 = 3*32^6 + 31*32^5 + ... + 31*32 + 31
    //            -> indices [3, 31, 31, 31, 31, 31, 31] -> 'd999999'
    expect(encodeZBase32(0xFFFFFFFFn)).toBe('d999999');
    expect(encodeZBase32(0xFFFFFFFFn).length).toBe(ZBASE32_MAX_STRLEN);
  });

  it('rejects negative values', () => {
    expect(() => encodeZBase32(-1n)).toThrow();
  });

  it('rejects values above the uint32 range', () => {
    expect(() => encodeZBase32(0x100000000n)).toThrow();
    expect(() => encodeZBase32(2n ** 64n)).toThrow();
  });

  it('produces only lowercase alphabet characters and digits', () => {
    for (const value of [0n, 1n, 31n, 32n, 1000n, 257899n, 257900n, 999999n, 0xFFFFFFFFn]) {
      const result = encodeZBase32(value);
      expect(result).toMatch(/^[a-z0-9]+$/);
      // z-base-32 specifically excludes '0', 'l', 'v', and '2'.
      expect(result).not.toMatch(/[0lv2]/);
    }
  });
});

describe('intToZBase32', () => {
  it('matches the BigInt-based encoder for small values', () => {
    expect(intToZBase32(0)).toBe('y');
    expect(intToZBase32(1)).toBe('b');
    expect(intToZBase32(31)).toBe('9');
    expect(intToZBase32(32)).toBe('by');
  });

  it('handles values above the UID threshold (257899)', () => {
    expect(intToZBase32(257900)).toBe('855c');
  });

  it('accepts the full uint32 range', () => {
    expect(intToZBase32(0)).toBe('y');
    expect(intToZBase32(0xFFFFFFFF)).toBe('d999999');
  });

  it('rejects negative numbers', () => {
    expect(() => intToZBase32(-1)).toThrow();
  });

  it('rejects values outside the uint32 range', () => {
    expect(() => intToZBase32(0x100000000)).toThrow();
    expect(() => intToZBase32(Number.MAX_SAFE_INTEGER)).toThrow();
  });

  it('rejects non-integer numbers', () => {
    expect(() => intToZBase32(Number.NaN)).toThrow();
    expect(() => intToZBase32(Number.POSITIVE_INFINITY)).toThrow();
    expect(() => intToZBase32(3.14)).toThrow();
  });
});
