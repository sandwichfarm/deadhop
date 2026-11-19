import { describe, expect, it } from 'vitest';
import { formatBtc, formatSats, receivedSummary, statusLabel, timeLeft } from './format.ts';

describe('format', () => {
  it('formats amounts', () => {
    expect(formatBtc(123_456_789)).toBe('1.23456789 BTC');
    expect(formatBtc(5000)).toBe('0.00005000 BTC');
    expect(formatSats(1_234_567)).toBe('1,234,567 sats');
    expect(formatSats(999)).toBe('999 sats');
  });

  it('counts down', () => {
    expect(timeLeft(100, 100)).toBe('');
    expect(timeLeft(100, 200)).toBe('');
    expect(timeLeft(3_600 + 120, 0)).toBe('1h 2m');
    expect(timeLeft(70, 0)).toBe('1m 10s');
  });

  it('labels statuses and summarises receipts', () => {
    expect(statusLabel('pending')).toBe('Awaiting payment');
    expect(statusLabel('detected')).toBe('Payment seen, confirming');
    expect(statusLabel('confirmed')).toBe('Paid');
    expect(statusLabel('expired')).toBe('Expired');
    expect(receivedSummary({ received_sat: 1000, confirmed_sat: 0, confirmations_required: 1 })).toBe(
      'Received 1,000 sats, confirmed 0 sats (needs 1 confirmation).',
    );
    expect(receivedSummary({ received_sat: 1000, confirmed_sat: 1000, confirmations_required: 3 })).toContain('3 confirmations');
  });
});
