import { test, expect } from '@playwright/test';
import { getChargerRelaySettings } from './common';

const cases = [
  { domain: 'https://relay.example.com', port: '', host: 'relay.example.com', expectedPort: '443' },
  { domain: 'https://relay.example.com:8443', port: '', host: 'relay.example.com', expectedPort: '8443' },
  { domain: 'https://relay.example.com', port: ':8081', host: 'relay.example.com', expectedPort: '8081' },
  { domain: 'https://relay.example.com', port: '8081', host: 'relay.example.com', expectedPort: '8081' },
  { domain: 'https://relay.example.com:8443', port: ':8081', host: 'relay.example.com', expectedPort: '8081' },
  { domain: 'https://relay.example.com:8443/path/', port: ':8081', host: 'relay.example.com', expectedPort: '8081' },
  { domain: 'https://192.0.2.1:8443/', port: '', host: '192.0.2.1', expectedPort: '8443' },
];

for (const { domain, port, host, expectedPort } of cases) {
  test(`charger relay settings for ${domain} with TEST_PORT=${port || '(unset)'}`, () => {
    expect(getChargerRelaySettings(domain, port)).toEqual({ host, port: expectedPort });
  });
}
