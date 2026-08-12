import { describe, it, expect } from 'vitest';
import { encodeBase58Flickr, encodeUid, ZBASE32_UID_THRESHOLD } from './base58';
import { intToZBase32 } from './zbase32';

// Alphabet used by encodeBase58Flickr for reference:
// '123456789abcdefghijkmnopqrstuvwxyzABCDEFGHJKLMNPQRSTUVWXYZ'

// Reference Flickr-base58 encoder used to cross-check encodeUid's output
// against a known-good implementation.
const BASE58_FLICKR_ALPHABET = '123456789abcdefghijkmnopqrstuvwxyzABCDEFGHJKLMNPQRSTUVWXYZ';

function referenceIntToBase58(num: number): string {
    let str = '';
    let n = num;
    while (n >= 58) {
        str = BASE58_FLICKR_ALPHABET[n % 58] + str;
        n = Math.floor(n / 58);
    }
    return BASE58_FLICKR_ALPHABET[n] + str;
}

describe('encodeBase58Flickr', () => {
  it('encodes empty input as "1"', () => {
    const input = new Uint8Array([]);
    expect(encodeBase58Flickr(input)).toBe('1');
  });

  it('encodes a single zero byte as "1"', () => {
    const input = new Uint8Array([0x00]);
    expect(encodeBase58Flickr(input)).toBe('1');
  });

  it('encodes multiple zero bytes as a single "1" (value is zero)', () => {
    const input = new Uint8Array([0x00, 0x00]);
    // value remains 0n -> early return
    expect(encodeBase58Flickr(input)).toBe('1');
  });

  it('preserves leading zeroes as leading "1" characters', () => {
    const input = new Uint8Array([0x00, 0xff]);
    // 0xff -> base58 Flickr is '5p' and one leading zero -> '1' prefix
    expect(encodeBase58Flickr(input)).toBe('15p');
  });

  it('encodes a non-zero single byte correctly', () => {
    const input = new Uint8Array([0xff]);
    // 255 -> 58*4=232 remainder 23 -> indices [4,23] => '5p'
    expect(encodeBase58Flickr(input)).toBe('5p');
  });

  it('encodes multiple bytes without leading zeros', () => {
    const input = new Uint8Array([0x01, 0x02]); // 258 decimal
    // 258 -> 58*4=232 remainder 26 -> indices [4,26] => '5s'
    expect(encodeBase58Flickr(input)).toBe('5s');
  });

  it('handles multiple leading zeroes (prefix with multiple 1s)', () => {
    const input = new Uint8Array([0x00, 0x00, 0x01]);
    // value=1 -> '2', with two leading zeros -> '112'
    expect(encodeBase58Flickr(input)).toBe('112');
  });

  it('encodes 58 correctly (two digits)', () => {
    const input = new Uint8Array([0x3a]); // 58 decimal
    // 58 -> digits [0,1] => '21' with Flickr alphabet
    expect(encodeBase58Flickr(input)).toBe('21');
  });

  it('encodes 256 correctly and with leading zeros', () => {
    const noLeading = new Uint8Array([0x01, 0x00]); // 256 decimal -> '5q'
    expect(encodeBase58Flickr(noLeading)).toBe('5q');

    const withLeading = new Uint8Array([0x00, 0x00, 0x01, 0x00]); // two leading zeros + 256 -> '115q'
    expect(encodeBase58Flickr(withLeading)).toBe('115q');
  });

  it('encodes with multiple leading zeros before 0xff (255)', () => {
    const input = new Uint8Array([0x00, 0x00, 0xff]);
    // base value 255 => '5p', two leading zeros -> prefix '11'
    expect(encodeBase58Flickr(input)).toBe('115p');
  });
});

describe('encodeUid', () => {
  it('uses base58 for UIDs at or below the threshold', () => {
    expect(encodeUid(0)).toBe(referenceIntToBase58(0));
    expect(encodeUid(1)).toBe(referenceIntToBase58(1));
    expect(encodeUid(12345)).toBe(referenceIntToBase58(12345));
    expect(encodeUid(ZBASE32_UID_THRESHOLD)).toBe(referenceIntToBase58(ZBASE32_UID_THRESHOLD));
  });

  it('uses z-base-32 for UIDs above the threshold', () => {
    expect(encodeUid(ZBASE32_UID_THRESHOLD + 1)).toBe(intToZBase32(ZBASE32_UID_THRESHOLD + 1));
    expect(encodeUid(1_000_000)).toBe(intToZBase32(1_000_000));
  });

  it('boundary: encodeUid(257899) is base58, encodeUid(257900) is z-base-32', () => {
    expect(encodeUid(257899)).toBe(referenceIntToBase58(257899));
    expect(encodeUid(257900)).toBe(intToZBase32(257900));
    expect(encodeUid(257899)).not.toBe(encodeUid(257900));
  });

  it('exposes the threshold as a module constant', () => {
    expect(ZBASE32_UID_THRESHOLD).toBe(257899);
  });
});
