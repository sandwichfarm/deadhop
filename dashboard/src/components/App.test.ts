import { afterEach, describe, expect, it, vi } from 'vitest';
import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import App from './App.svelte';
import type { PayApi, Session } from '../lib/api';
import { nip19 } from 'nostr-tools';
import { PUBKEY, health, info, invoice, metrics, series, settings, settingsView, stats, privacyAudit } from '../test-helpers';

const config = { serverPubkey: PUBKEY, relays: ['ws://127.0.0.1:7777'] };

function session(over: Parameters<typeof info>[0] = {}, api: Record<string, unknown> = {}): Session {
  return {
    api: {
      getInvoice: vi.fn().mockResolvedValue(invoice()),
      createInvoice: vi.fn().mockResolvedValue(invoice()),
      getStats: vi.fn().mockResolvedValue(stats()),
      listInvoices: vi.fn().mockResolvedValue({ invoices: [], next_before: null }),
      getHealth: vi.fn().mockResolvedValue(health()),
      getPrivacyAudit: vi.fn().mockResolvedValue(privacyAudit()),
      getMetrics: vi.fn().mockResolvedValue(metrics()),
      getSeries: vi.fn().mockResolvedValue(series()),
      getSettings: vi.fn().mockResolvedValue(settingsView()),
      updateSettings: vi.fn().mockResolvedValue(settingsView({ settings: settings({ pow_bits: 18 }) })),
      getInfo: vi.fn().mockResolvedValue(info({ is_admin: true, name: 'Renamed shop' })),
      ...api,
    } as unknown as PayApi,
    pubkey: PUBKEY,
    info: info(over),
    close: vi.fn().mockResolvedValue(undefined),
  };
}

function setup(connect: ReturnType<typeof vi.fn>, extension: () => unknown = () => undefined) {
  const loadConfig = vi.fn().mockResolvedValue(config);
  const view = render(App, { props: { loadConfig, connect, extension } as never });
  return { loadConfig, connect, ...view };
}

afterEach(() => {
  location.hash = '';
});

describe('App', () => {
  it('stops with an explanation when the configuration cannot be loaded', async () => {
    render(App, { props: { loadConfig: vi.fn().mockRejectedValue(new Error('config.json is invalid')), connect: vi.fn() } as never });
    expect(screen.getByRole('status').textContent).toBe('Loading…');
    await waitFor(() => expect(screen.getByRole('alert').textContent).toBe('config.json is invalid'));
  });

  it('signs a guest in and offers public invoicing, then opens the checkout', async () => {
    const s = session();
    const t = setup(vi.fn().mockResolvedValue(s));
    await fireEvent.click(await screen.findByRole('button', { name: 'Continue as guest' }));
    await screen.findByRole('heading', { name: 'New invoice' });
    expect(screen.getByText(/which is not an operator of this service/).textContent).toContain('Settings → Operators');
    expect(t.connect).toHaveBeenCalledTimes(1);
    expect(t.connect.mock.calls[0][1]).toEqual(config);
    expect(screen.getByText('regtest')).toBeTruthy();
    // The footer shows the service key whole, not shortened.
    expect(screen.getByText(nip19.npubEncode(PUBKEY))).toBeTruthy();
    expect(screen.getByTitle(PUBKEY)).toBeTruthy();

    await fireEvent.input(screen.getByLabelText('Amount (sats)'), { target: { value: '50000' } });
    await fireEvent.click(screen.getByRole('button', { name: 'Create invoice' }));
    await waitFor(() => expect(location.hash).toBe(`#/pay/${invoice().id}`));
    window.dispatchEvent(new HashChangeEvent('hashchange'));
    await screen.findByText('0.00050000 BTC');
    expect(s.api.getInvoice).toHaveBeenCalledWith(invoice().id);
  });

  it('signs in with a pasted key and shows the operator view', async () => {
    const t = setup(vi.fn().mockResolvedValue(session({ is_admin: true, name: 'Corner shop' })));
    await fireEvent.input(await screen.findByLabelText(/paste a secret key/), { target: { value: '1'.repeat(64) } });
    await fireEvent.click(screen.getByRole('button', { name: 'Sign in' }));
    await screen.findByRole('heading', { name: 'Relays' });
    expect(screen.getByRole('link', { name: 'Corner shop' })).toBeTruthy();
    const signer = t.connect.mock.calls[0][0];
    expect(await signer.getPublicKey()).toMatch(/^[0-9a-f]{64}$/);

    await fireEvent.click(screen.getByRole('button', { name: 'Invoices' }));
    await screen.findByRole('heading', { name: 'Drops' });

    // A settings change refreshes what the page knows about the service.
    await fireEvent.click(screen.getByRole('button', { name: 'Settings' }));
    await fireEvent.input(await screen.findByLabelText('Proof of work (bits)'), { target: { value: '18' } });
    await fireEvent.click(screen.getByRole('button', { name: 'Save changes' }));
    await screen.findByRole('link', { name: 'Renamed shop' });
  });

  it('opens the payment page of an invoice an operator creates', async () => {
    setup(vi.fn().mockResolvedValue(session({ is_admin: true })));
    await fireEvent.input(await screen.findByLabelText(/paste a secret key/), { target: { value: '1'.repeat(64) } });
    await fireEvent.click(screen.getByRole('button', { name: 'Sign in' }));
    await fireEvent.click(await screen.findByRole('button', { name: 'Invoices' }));
    await fireEvent.click(screen.getByRole('button', { name: 'New invoice' }));
    await fireEvent.input(screen.getByLabelText('Amount (sats)'), { target: { value: '50000' } });
    await fireEvent.click(screen.getByRole('button', { name: 'Create invoice' }));
    await waitFor(() => expect(location.hash).toBe(`#/pay/${invoice().id}`));
  });

  it('keeps the old service details when refreshing them fails', async () => {
    const s = session({ is_admin: true, name: 'Corner shop' }, { getInfo: vi.fn().mockRejectedValue(new Error('gone')) });
    setup(vi.fn().mockResolvedValue(s));
    await fireEvent.input(await screen.findByLabelText(/paste a secret key/), { target: { value: '1'.repeat(64) } });
    await fireEvent.click(screen.getByRole('button', { name: 'Sign in' }));
    await fireEvent.click(await screen.findByRole('button', { name: 'Settings' }));
    await fireEvent.input(await screen.findByLabelText('Proof of work (bits)'), { target: { value: '18' } });
    await fireEvent.click(screen.getByRole('button', { name: 'Save changes' }));
    await screen.findByText('Saved.');
    expect(s.api.getInfo).toHaveBeenCalled();
    expect(screen.getByRole('link', { name: 'Corner shop' })).toBeTruthy();
  });

  it('signs in with a browser extension', async () => {
    const ext = {
      getPublicKey: vi.fn().mockResolvedValue(PUBKEY),
      signEvent: vi.fn(),
      nip44: { encrypt: vi.fn(), decrypt: vi.fn() },
    };
    const t = setup(vi.fn().mockResolvedValue(session({ public_invoicing: false })), () => ext);
    await fireEvent.click(await screen.findByRole('button', { name: 'Use browser extension' }));
    await screen.findByText(/not an operator of this service/);
    expect(await t.connect.mock.calls[0][0].getPublicKey()).toBe(PUBKEY);
  });

  it('explains sign-in problems and lets the user retry', async () => {
    const connect = vi.fn().mockRejectedValueOnce(new Error('Could not reach the service.')).mockResolvedValue(session());
    setup(connect);
    await fireEvent.click(await screen.findByRole('button', { name: 'Use browser extension' }));
    await waitFor(() => expect(screen.getByRole('alert').textContent).toContain('No Nostr extension'));
    expect(connect).not.toHaveBeenCalled();

    await fireEvent.input(screen.getByLabelText(/paste a secret key/), { target: { value: 'hunter2' } });
    await fireEvent.click(screen.getByRole('button', { name: 'Sign in' }));
    await waitFor(() => expect(screen.getByRole('alert').textContent).toContain('Enter an nsec'));

    await fireEvent.click(screen.getByRole('button', { name: 'Continue as guest' }));
    await waitFor(() => expect(screen.getByRole('alert').textContent).toBe('Could not reach the service.'));
    await fireEvent.click(screen.getByRole('button', { name: 'Continue as guest' }));
    await screen.findByRole('heading', { name: 'New invoice' });
  });

  it('signs out and closes the connection even if closing fails', async () => {
    const s = session();
    (s.close as ReturnType<typeof vi.fn>).mockRejectedValue(new Error('already closed'));
    setup(vi.fn().mockResolvedValue(s));
    await fireEvent.click(await screen.findByRole('button', { name: 'Continue as guest' }));
    await fireEvent.click(await screen.findByRole('button', { name: 'Sign out' }));
    await screen.findByRole('heading', { name: 'Sign in with Nostr' });
    expect(s.close).toHaveBeenCalledTimes(1);
    expect(screen.getByRole('link', { name: 'deadhop' })).toBeTruthy();
  });

  it('opens a payment link without asking the visitor to sign in', async () => {
    location.hash = `#/pay/${invoice().id}`;
    const t = setup(vi.fn().mockResolvedValue(session()));
    await screen.findByText('0.00050000 BTC');
    expect(t.connect).toHaveBeenCalledTimes(1);
    expect(screen.queryByRole('heading', { name: 'Sign in with Nostr' })).toBeNull();

    // Going back home keeps the session.
    location.hash = '#/';
    window.dispatchEvent(new HashChangeEvent('hashchange'));
    await screen.findByRole('heading', { name: 'New invoice' });
  });

  it('falls back to the sign-in screen when a payment link cannot connect', async () => {
    location.hash = `#/pay/${invoice().id}`;
    let fail!: (e: Error) => void;
    const connect = vi.fn(() => new Promise((_, reject) => (fail = reject)));
    setup(connect);
    await waitFor(() => expect(connect).toHaveBeenCalledTimes(1));
    expect(screen.getByRole('status').textContent).toBe('Connecting…');
    fail(new Error('Could not reach the service.'));
    await waitFor(() => expect(screen.getByRole('alert').textContent).toBe('Could not reach the service.'));
    expect(connect).toHaveBeenCalledTimes(1);
  });

  it('stops listening for navigation when removed and uses real defaults', async () => {
    const remove = vi.spyOn(window, 'removeEventListener');
    vi.spyOn(globalThis, 'fetch').mockRejectedValue(new Error('offline'));
    const view = render(App);
    await waitFor(() => expect(screen.getByRole('alert').textContent).toContain('config.json is missing'));
    view.unmount();
    expect(remove).toHaveBeenCalledWith('hashchange', expect.any(Function));
  });

  it('reads the extension from window.nostr by default', async () => {
    vi.spyOn(globalThis, 'fetch').mockResolvedValue({ ok: true, json: async () => config } as Response);
    render(App);
    await fireEvent.click(await screen.findByRole('button', { name: 'Use browser extension' }));
    await waitFor(() => expect(screen.getByRole('alert').textContent).toContain('No Nostr extension'));
  });
});
