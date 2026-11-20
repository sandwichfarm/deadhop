import { describe, expect, it } from 'vitest';
import { dropId, formatBtc, formatBytes, formatDuration, formatSats, formatTime, receivedSummary, shortKey, statusLabel, timeLeft } from './format';
import { PUBKEY } from '../test-helpers';

describe('format', () => {
  it('formats amounts', () => {
    expect(formatBtc(1)).toBe('0.00000001 BTC');
    expect(formatBtc(123_456_789)).toBe('1.23456789 BTC');
    expect(formatSats(0)).toBe('0 sats');
    expect(formatSats(999)).toBe('999 sats');
    expect(formatSats(1_234_567)).toBe('1,234,567 sats');
  });

  it('formats times', () => {
    expect(formatTime(1_700_000_000)).toBe('2023-11-14 22:13 UTC');
    expect(timeLeft(100, 100)).toBe('');
    expect(timeLeft(100, 200)).toBe('');
    expect(timeLeft(165, 100)).toBe('1m 5s');
    expect(timeLeft(100 + 2 * 3600 + 180, 100)).toBe('2h 3m');
  });

  it('summarises what has been received', () => {
    expect(receivedSummary({ received_sat: 50_000, confirmed_sat: 0, confirmations_required: 2 })).toBe(
      'Received 50,000 sats, confirmed 0 sats (needs 2 confirmations).',
    );
    expect(receivedSummary({ received_sat: 50_000, confirmed_sat: 50_000, confirmations_required: 1 })).toBe(
      'Received 50,000 sats, confirmed 50,000 sats (needs 1 confirmation).',
    );
  });

  it('shortens keys and labels statuses', () => {
    expect(shortKey(PUBKEY)).toBe('79be667e…f81798');
    expect(shortKey('abcd')).toBe('abcd');
    expect(statusLabel('pending')).toBe('Awaiting payment');
    expect(statusLabel('detected')).toBe('Payment seen, confirming');
    expect(statusLabel('confirmed')).toBe('Paid');
    expect(statusLabel('expired')).toBe('Expired');
  });
});

describe('durations and sizes', () => {
  it('formats them compactly', () => {
    expect(formatDuration(59)).toBe('59s');
    expect(formatDuration(120)).toBe('2m');
    expect(formatDuration(3_700)).toBe('1h 1m');
    expect(formatDuration(93_784)).toBe('1d 2h');
    expect(formatBytes(512)).toBe('512 B');
    expect(formatBytes(1536)).toBe('1.5 KB');
    expect(formatBytes(5 * 1024 * 1024)).toBe('5.0 MB');
    expect(formatBytes(3 * 1024 ** 4)).toBe('3072.0 GB');
  });
});

describe('drop ids', () => {
  it('are the address index in hex, padded to 4 digits', () => {
    expect(dropId(0x7f3a)).toBe('7F3A');
    expect(dropId(1)).toBe('0001');
    expect(dropId(0x12345)).toBe('12345');
  });
});
