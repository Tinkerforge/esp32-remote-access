import { test, expect } from '@playwright/test';
import { encodeBase58Flickr } from '../src/base58';

test('encode empty Uint8Array', () => {
  // The empty Uint8Array will be converted to a value of 0, so expect the first alphabet character.
  const result = encodeBase58Flickr(new Uint8Array([]));
  expect(result).toBe('1');
});

test('encode Uint8Array with a single zero byte', () => {
  const result = encodeBase58Flickr(new Uint8Array([0]));
  expect(result).toBe('1');
});

test('encode Uint8Array [1,2,3]', () => {
  // Expected computed manually: [1,2,3] => 'kCP'
  const result = encodeBase58Flickr(new Uint8Array([1, 2, 3]));
  expect(result).toBe('kCP');
});

// ...additional tests as needed...
