import { describe, expect, it } from 'vitest';
import { errorRate, statsFor } from './relays';
import { relayHistory, relayStats } from '../test-helpers';

describe('relay history helpers', () => {
  it('match a relay by address, with or without a trailing slash', () => {
    const history = relayHistory({ relays: [relayStats({ relay: 'wss://a.example/' }), relayStats({ relay: 'wss://b.example' })] });
    expect(statsFor(history, 'wss://a.example')?.relay).toBe('wss://a.example/');
    expect(statsFor(history, 'wss://b.example/')?.relay).toBe('wss://b.example');
    expect(statsFor(history, 'wss://c.example')).toBeNull();
    expect(statsFor(null, 'wss://a.example')).toBeNull();
  });

  it('turn failures into a rate and a colour', () => {
    expect(errorRate(relayStats({ checks: 0, sends: 0, check_failures: 0, send_failures: 0 }))).toMatchObject({ text: '—', tone: 'none' });
    expect(errorRate(relayStats({ checks: 5, sends: 15, check_failures: 0, send_failures: 0 }))).toMatchObject({ text: '0%', tone: 'ok' });
    expect(errorRate(relayStats({ checks: 10, sends: 30, check_failures: 1, send_failures: 0 }))).toMatchObject({ failed: 1, total: 40, text: '2.5%', tone: 'warn' });
    expect(errorRate(relayStats({ checks: 5, sends: 5, check_failures: 1, send_failures: 1 }))).toMatchObject({ text: '20%', tone: 'bad' });
    expect(errorRate(relayStats({ checks: 4, sends: 0, check_failures: 4, send_failures: 0 }))).toMatchObject({ text: '100%', tone: 'bad' });
  });
});
