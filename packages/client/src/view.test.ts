import { describe, expect, it } from 'vitest';
import { LABELS, invoiceView, parseSats, payLabel, progressMessage } from './view.ts';
import { invoice } from './test-helpers.ts';
import { ApiError } from './api.ts';

describe('invoiceView', () => {
  it('shows a payable invoice with QR, address and countdown', () => {
    const v = invoiceView(invoice(), 1_700_000_000);
    expect(v).toMatchObject({
      status: 'pending',
      statusLabel: 'Awaiting payment',
      amountBtc: '0.00050000 BTC',
      amountSats: '50,000 sats',
      memo: 'table 4',
      payable: true,
      expiry: 'Expires in 1h 0m',
      received: null,
    });
    expect(v.qr?.path.length).toBeGreaterThan(0);
  });

  it('hides payment details once settled or expired, and summarises receipts', () => {
    const paid = invoiceView(invoice({ status: 'confirmed', received_sat: 50_000, confirmed_sat: 50_000, memo: '' }), 0);
    expect(paid).toMatchObject({ payable: false, qr: null, memo: null, expiry: '' });
    expect(paid.received).toContain('Received 50,000 sats');
    expect(invoiceView(invoice({ status: 'detected', received_sat: 1 }), 0)).toMatchObject({ payable: true, expiry: '' });
    expect(invoiceView(invoice(), 1_800_000_000).expiry).toBe('');
    expect(invoiceView(invoice({ status: 'expired' }), 0).payable).toBe(false);
  });
});

describe('helpers', () => {
  it('describe progress, label buttons and parse amounts', () => {
    const base = { invoice: null, error: null, now: 0 };
    expect(progressMessage({ ...base, phase: 'creating' })).toBe('Preparing invoice…');
    expect(progressMessage({ ...base, phase: 'loading' })).toBe('Loading invoice…');
    expect(progressMessage({ ...base, phase: 'error', error: new ApiError('network') })).toBeNull();
    expect(payLabel(5000)).toBe('Pay 5,000 sats');
    expect(LABELS.pay(21)).toBe('Pay 21 sats');
    expect(parseSats('5000')).toBe(5000);
    expect(parseSats(' 5,000 ')).toBe(5000);
    expect(parseSats('5 000_000')).toBe(5_000_000);
    for (const bad of ['', '0', '-1', '1.5', 'abc', '1e3', '99999999999999999']) expect(parseSats(bad), bad).toBeNull();
  });
});
