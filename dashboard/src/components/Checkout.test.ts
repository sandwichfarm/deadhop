import { describe, expect, it, vi } from 'vitest';
import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import Checkout from './Checkout.svelte';
import type { PayApi } from '../lib/api';
import { ADDRESS, invoice } from '../test-helpers';

const ID = invoice().id;

function setup(getInvoice: ReturnType<typeof vi.fn>, extra: Record<string, unknown> = {}) {
  const copy = vi.fn().mockResolvedValue(undefined);
  const view = render(Checkout, {
    props: { api: { getInvoice } as unknown as PayApi, id: ID, pollMs: 15, clock: () => 1_700_000_000, copy, ...extra },
  });
  return { copy, getInvoice, ...view };
}

describe('Checkout', () => {
  it('shows a payable invoice with QR code, address and expiry', async () => {
    const t = setup(vi.fn().mockResolvedValue(invoice()));
    expect(screen.getByRole('status').textContent).toBe('Loading invoice…');
    await waitFor(() => expect(screen.getByRole('heading').textContent).toBe('0.00050000 BTC'));
    expect(t.getInvoice).toHaveBeenCalledWith(ID);
    expect(screen.getByRole('status').textContent).toBe('Awaiting payment');
    expect(screen.getByText('50,000 sats')).toBeTruthy();
    expect(screen.getByText('table 4')).toBeTruthy();
    expect(screen.getByText('Note from the invoice creator:')).toBeTruthy();
    expect(screen.getByText(ADDRESS)).toBeTruthy();
    expect(screen.getByText('Expires in 1h 0m')).toBeTruthy();
    const link = screen.getByRole('link', { name: 'Open in wallet' }) as HTMLAnchorElement;
    expect(link.getAttribute('href')).toBe(invoice().bip21);
    expect(screen.getByRole('img', { name: 'Payment QR code' })).toBeTruthy();
    expect(screen.queryByText(/Received/)).toBeNull();

    await fireEvent.click(screen.getByRole('button', { name: 'Copy address' }));
    expect(t.copy).toHaveBeenCalledWith(ADDRESS);
    await waitFor(() => expect(screen.getByRole('button', { name: 'Copied' })).toBeTruthy());
  });

  it('follows the payment until it is confirmed, then stops polling', async () => {
    const getInvoice = vi
      .fn()
      .mockResolvedValueOnce(invoice({ memo: '' }))
      .mockResolvedValueOnce(invoice({ memo: '', status: 'detected', received_sat: 50_000, confirmations_required: 2 }))
      .mockResolvedValue(invoice({ memo: '', status: 'confirmed', received_sat: 50_000, confirmed_sat: 50_000, settled_at: 1_700_000_900 }));
    setup(getInvoice);
    await waitFor(() => expect(screen.getByRole('status').textContent).toBe('Payment seen, confirming'));
    expect(screen.queryByText('Note from the invoice creator:')).toBeNull();
    expect(screen.getByText('Received 50,000 sats, confirmed 0 sats (needs 2 confirmations).')).toBeTruthy();
    // Still payable while confirming, but the expiry countdown is gone.
    expect(screen.getByRole('img', { name: 'Payment QR code' })).toBeTruthy();
    expect(screen.queryByText(/Expires in/)).toBeNull();

    await waitFor(() => expect(screen.getByRole('status').textContent).toBe('Paid'));
    expect(screen.getByText('Received 50,000 sats, confirmed 50,000 sats (needs 1 confirmation).')).toBeTruthy();
    expect(screen.queryByRole('img')).toBeNull();
    expect(screen.queryByRole('button')).toBeNull();
    const calls = getInvoice.mock.calls.length;
    await new Promise((r) => setTimeout(r, 60));
    expect(getInvoice.mock.calls.length).toBe(calls);
  });

  it('shows an expired invoice without payment details', async () => {
    setup(vi.fn().mockResolvedValue(invoice({ status: 'expired' })));
    await waitFor(() => expect(screen.getByRole('status').textContent).toBe('Expired'));
    expect(screen.queryByRole('img')).toBeNull();
    expect(screen.queryByText(ADDRESS)).toBeNull();
  });

  it('hides the countdown once the expiry time has passed', async () => {
    setup(vi.fn().mockResolvedValue(invoice()), { clock: () => 1_700_009_999 });
    await waitFor(() => expect(screen.getByText(ADDRESS)).toBeTruthy());
    expect(screen.queryByText(/Expires in/)).toBeNull();
  });

  it('reports a failed first load', async () => {
    setup(vi.fn().mockRejectedValue(new Error('Invoice not found.')));
    await waitFor(() => expect(screen.getByRole('alert').textContent).toBe('Invoice not found.'));
    expect(screen.queryByRole('heading')).toBeNull();
  });

  it('keeps showing the invoice when a later refresh fails, and recovers', async () => {
    const getInvoice = vi
      .fn()
      .mockResolvedValueOnce(invoice())
      .mockRejectedValueOnce(new Error('Could not reach the service.'))
      .mockResolvedValue(invoice());
    setup(getInvoice);
    await waitFor(() => expect(screen.getByRole('alert').textContent).toBe('Could not reach the service.'));
    expect(screen.getByRole('heading').textContent).toBe('0.00050000 BTC');
    await waitFor(() => expect(screen.queryByRole('alert')).toBeNull());
  });

  it('stops polling when removed, and uses the browser clock and clipboard by default', async () => {
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.defineProperty(navigator, 'clipboard', { value: { writeText }, configurable: true });
    const getInvoice = vi.fn().mockResolvedValue(invoice({ expires_at: Math.floor(Date.now() / 1000) + 600 }));
    const view = render(Checkout, { props: { api: { getInvoice } as unknown as PayApi, id: ID } });
    await waitFor(() => expect(screen.getByText(/Expires in 9m|Expires in 10m/)).toBeTruthy());
    await fireEvent.click(screen.getByRole('button', { name: 'Copy address' }));
    expect(writeText).toHaveBeenCalledWith(ADDRESS);
    view.unmount();
    const calls = getInvoice.mock.calls.length;
    await new Promise((r) => setTimeout(r, 30));
    expect(getInvoice.mock.calls.length).toBe(calls);
  });
});
