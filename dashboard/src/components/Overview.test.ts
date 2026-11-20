import { describe, expect, it, vi } from 'vitest';
import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import Overview from './Overview.svelte';
import type { PayApi } from '../lib/api';
import { PUBKEY, adminInvoice, health, metrics, privacyAudit, proxyCheck, series } from '../test-helpers';

type Fn = ReturnType<typeof vi.fn>;

function setup(
  over: Partial<Record<'getHealth' | 'getMetrics' | 'getSeries' | 'getPrivacyAudit' | 'listInvoices' | 'control', Fn>> = {},
  props: { refreshMs?: number; onAudit?: Fn; onDrops?: Fn; confirm?: Fn } = {},
) {
  const api = {
    getHealth: vi.fn().mockResolvedValue(health()),
    getMetrics: vi.fn().mockResolvedValue(metrics()),
    getSeries: vi.fn().mockResolvedValue(series()),
    getPrivacyAudit: vi.fn().mockResolvedValue(privacyAudit()),
    control: vi.fn().mockResolvedValue({ paused: false, restart_pending: true }),
    ...over,
  };
  const all = { refreshMs: 60_000, onAudit: vi.fn(), onDrops: vi.fn(), confirm: vi.fn().mockReturnValue(true), ...props };
  render(Overview, { props: { api: api as unknown as PayApi, serverPubkey: PUBKEY, ...all } as never });
  return { ...api, ...all };
}

const text = (label: string) => screen.getByText(label, { selector: '.lbl' }).parentElement!.textContent!.replace(/\s+/g, ' ').trim();

describe('Overview', () => {
  it('heads the page while connecting, then shows health, totals, windows and the chart', async () => {
    const api = setup();
    expect(screen.getByRole('heading', { level: 1 }).textContent).toBe('Connecting…');
    await screen.findByRole('heading', { name: 'Serving. Nothing to reach.' });
    expect(screen.getByText('Running')).toBeTruthy();
    expect(screen.getByText('up 1h 1m · v0.1.0 · 0 restarts')).toBeTruthy();
    expect(screen.getByText('1 of 1 connected')).toBeTruthy();
    expect(screen.getByText('direct connection')).toBeTruthy();
    expect(screen.getByText('Block 101')).toBeTruthy();
    expect(screen.getByText('plain TCP · polled 2023-11-14 22:15 UTC')).toBeTruthy();
    expect(screen.getByText('Healthy')).toBeTruthy();
    expect(screen.getByText('2.0 KB · never backed up')).toBeTruthy();
    expect(text('Addresses created')).toBe('Addresses created 12 never reused');
    expect(text('Received')).toBe('Received 0.00150000 BTC 150,000 sats');
    expect(text('Payments')).toBe('Payments 3 1 in the last 24 hours');
    expect(text('Invoices')).toBe('Invoices 10 10 in the database');
    expect(screen.getByText('Last 24 hours')).toBeTruthy();
    expect(screen.getAllByText('3 so far')).toHaveLength(2);
    expect(screen.getByText('150,000 sats; complete after 30d 0h online')).toBeTruthy();
    expect(screen.getByText('off').className).toBe('badge expired');
    expect(screen.getByRole('img').getAttribute('aria-label')).toContain('100,000 sats in 7 buckets');
    expect(api.getSeries).toHaveBeenCalledWith('week');
    // The service key is one line at the bottom, with the service's relays in its nprofile.
    expect(screen.getByRole('region', { name: 'Service key' })).toBeTruthy();

    await fireEvent.click(screen.getByRole('button', { name: '1 year' }));
    await waitFor(() => expect(api.getSeries).toHaveBeenLastCalledWith('year'));
    expect(screen.getByRole('button', { name: '1 year' }).getAttribute('aria-pressed')).toBe('true');
    await fireEvent.change(screen.getByLabelText('Show'), { target: { value: 'count' } });
    await waitFor(() => expect(screen.getByRole('img').getAttribute('aria-label')).toContain('2 payments'));
  });

  it('reports trouble in every part', async () => {
    setup({
      getHealth: vi.fn().mockResolvedValue(
        health({
          service: { version: '0.1.0', started_at: 1, uptime_secs: 59, online_since: 1, paused: true, restarts: 1, restart_pending: true },
          relays: [
            { url: 'wss://a.example', status: 'disconnected', connected: false },
            { url: 'wss://b.example', status: 'connecting', connected: false },
          ],
          proxy: 'socks5://u:***@127.0.0.1:9050',
          chain: { ok: false, tip_height: null, last_poll_at: null, poll_secs: 10, watch_errors: 2, backend_local: false, transport: 'tls-pinned' },
          database: { ok: false, size_bytes: 10, invoices: 0, backups: 3, last_backup_at: 1_700_000_000 },
          warnings: ['invoicing is stopped'],
        }),
      ),
      getMetrics: vi.fn().mockResolvedValue(metrics({ sweep: { enabled: true, mode: 'coinjoin', threshold_sat: 100_000, received_sat: 150_000, due: true } })),
    });
    await screen.findByRole('heading', { name: 'Invoicing stopped.' });
    expect(screen.getByText('Invoicing stopped')).toBeTruthy();
    expect(screen.getByText('up 59s · v0.1.0 · 1 restart · restarting…')).toBeTruthy();
    expect(screen.getByText('0 of 2 connected')).toBeTruthy();
    expect(screen.getByText('through the proxy')).toBeTruthy();
    expect(screen.getByText('Unreachable')).toBeTruthy();
    expect(screen.getByText('TLS, pinned certificate · not polled yet')).toBeTruthy();
    expect(screen.getByText('Integrity check failed')).toBeTruthy();
    expect(screen.getByText('10 B · backed up 2023-11-14 22:13 UTC')).toBeTruthy();
    expect(screen.getByText('2 invoices could not be read from the chain.')).toBeTruthy();
    expect(screen.getByText('The Electrum server is not on this machine.')).toBeTruthy();
    expect(screen.getByText('invoicing is stopped').className).toBe('warning');
    expect(screen.getByText(/CoinJoin above 0.00100000 BTC/)).toBeTruthy();
    expect(screen.getByText('The threshold is reached: a sweep is due.').className).toBe('warning');
    expect(screen.getByText('on').className).toBe('badge confirmed');
    const relays = screen.getByRole('region', { name: 'Relays' });
    expect([...relays.querySelectorAll('.badge')].map((b) => b.className)).toEqual(['badge expired', 'badge expired']);
    expect(relays.querySelector('.dot')!.className).toBe('dot bad');
    expect(screen.getByRole('button', { name: 'Start invoicing' })).toBeTruthy();
  });

  it('handles a single unreadable invoice, an unknown height, a sweep below its threshold and errors', async () => {
    setup({
      getHealth: vi.fn().mockResolvedValue(
        health({ chain: { ok: true, tip_height: null, last_poll_at: null, poll_secs: 10, watch_errors: 1, backend_local: true, transport: 'tls' } }),
      ),
      getMetrics: vi.fn().mockResolvedValue(metrics({ sweep: { enabled: true, mode: 'payjoin', threshold_sat: 9_000_000, received_sat: 1, due: false } })),
    });
    await screen.findByText('1 invoice could not be read from the chain.');
    expect(screen.getByText('Connected')).toBeTruthy();
    expect(screen.getByText('TLS (public authorities) · not polled yet')).toBeTruthy();
    expect(screen.getByText(/PayJoin above/)).toBeTruthy();
    expect(screen.getByText('Below the threshold.').className).toBe('muted');

    document.body.innerHTML = '';
    setup({ getHealth: vi.fn().mockRejectedValue(new Error('The service had an internal error.')) });
    await waitFor(() => expect(screen.getByRole('alert').textContent).toBe('The service had an internal error.'));
    expect(screen.getByRole('heading', { level: 1 }).textContent).toBe('Connecting…');
    expect(screen.queryByRole('button', { name: 'Restart relays' })).toBeNull();
  });

  it('refreshes on a timer', async () => {
    const api = setup({}, { refreshMs: 20 });
    await waitFor(() => expect(api.getHealth.mock.calls.length).toBeGreaterThan(2));
  });

  it('restarts the relays and stops or starts invoicing from the page head', async () => {
    const t = setup();
    await fireEvent.click(await screen.findByRole('button', { name: 'Restart relays' }));
    await waitFor(() => expect(t.control).toHaveBeenCalledWith('restart'));
    expect((await screen.findByRole('status')).textContent).toContain('Reconnecting to the relays');

    t.confirm.mockReturnValueOnce(false);
    await fireEvent.click(screen.getByRole('button', { name: 'Stop invoicing' }));
    expect(t.control).toHaveBeenCalledTimes(1);
    await fireEvent.click(screen.getByRole('button', { name: 'Stop invoicing' }));
    await waitFor(() => expect(t.control).toHaveBeenCalledWith('stop'));
    await screen.findByText(/Invoicing stopped. Payments/);

    document.body.innerHTML = '';
    const u = setup({
      getHealth: vi.fn().mockResolvedValue(health({ service: { ...health().service, paused: true } })),
      control: vi.fn().mockRejectedValue(new Error('A request was refused.')),
    });
    await fireEvent.click(await screen.findByRole('button', { name: 'Start invoicing' }));
    await waitFor(() => expect(u.control).toHaveBeenCalledWith('start'));
    await waitFor(() => expect(screen.getByRole('alert').textContent).toBe('A request was refused.'));
  });

  it('asks the browser to confirm a stop by default', async () => {
    const ask = vi.spyOn(window, 'confirm').mockReturnValue(false);
    const api = { getHealth: vi.fn().mockResolvedValue(health()), getMetrics: vi.fn().mockResolvedValue(metrics()), getSeries: vi.fn().mockResolvedValue(series()), getPrivacyAudit: vi.fn().mockRejectedValue(new Error('x')), control: vi.fn() };
    render(Overview, { props: { api: api as unknown as PayApi, serverPubkey: PUBKEY, onAudit: vi.fn(), onDrops: vi.fn() } });
    await fireEvent.click(await screen.findByRole('button', { name: 'Stop invoicing' }));
    expect(ask).toHaveBeenCalled();
    expect(api.control).not.toHaveBeenCalled();
    ask.mockRestore();
  });

  it('shows the privacy score as a ring, which opens the audit', async () => {
    const t = setup();
    const tile = await screen.findByRole('button', { name: 'Privacy score 90 out of 100. Open the privacy audit' });
    expect(tile.textContent).toContain('1 thing to fix');
    expect(tile.textContent).toContain('8 of 10 checks could run here.');
    expect(tile.querySelector('svg')!.getAttribute('class')).toBe('ring small ok');
    await fireEvent.click(tile);
    expect(t.onAudit).toHaveBeenCalled();
  });

  it('colours the score and counts what to fix', async () => {
    setup({ getPrivacyAudit: vi.fn().mockResolvedValue(privacyAudit({ score: 70, checks: [] })) });
    expect((await screen.findByText('Nothing to fix')).className).toBe('big warn');

    document.body.innerHTML = '';
    const two = privacyAudit().checks.map((c) => ({ ...c, result: 'fail' as const }));
    setup({ getPrivacyAudit: vi.fn().mockResolvedValue(privacyAudit({ score: 40, checks: two })) });
    expect((await screen.findByText('4 things to fix')).className).toBe('big bad');
  });

  it('leaves the score out when the audit cannot be read', async () => {
    setup({ getPrivacyAudit: vi.fn().mockRejectedValue(new Error('unauthorized')) });
    await screen.findByText('Running');
    expect(screen.queryByText(/Privacy score/)).toBeNull();
  });

  it('shows the proxy, how fast its last check was, and when it ran', async () => {
    setup({ getHealth: vi.fn().mockResolvedValue(health({ proxy: 'socks5://127.0.0.1:9050', proxy_check: proxyCheck() })) });
    const tile = (await screen.findByText('online')).closest('.tile')!;
    expect(tile.querySelector('.big')!.textContent).toMatch(/^Reachable · \d+ ms$/);
    expect(tile.textContent).toContain('socks5://127.0.0.1:9050 · last check connected');
    expect(tile.textContent).toContain('1 of 1 relays through it: they see the proxy, not this machine. Checked ');
  });

  it('says reachable without a time when nothing was measured, and not connected when no relay is', async () => {
    setup({ getHealth: vi.fn().mockResolvedValue(health({ proxy: 'socks5://127.0.0.1:9050' })) });
    const tile = (await screen.findByText('online')).closest('.tile')!;
    expect(tile.querySelector('.big')!.textContent).toBe('Reachable');
    expect(tile.textContent).toContain('Not checked yet.');

    document.body.innerHTML = '';
    const failed = proxyCheck({ status: 'failed', relays: [{ relay: 'ws://127.0.0.1:7777', ok: false, code: 'ECONNREFUSED', reason: 'refused', ms: null }] });
    setup({
      getHealth: vi.fn().mockResolvedValue(
        health({ proxy: 'socks5://127.0.0.1:9050', proxy_check: failed, relays: [{ url: 'ws://127.0.0.1:7777', status: 'connecting', connected: false }] }),
      ),
    });
    expect((await screen.findByText('Not connected')).className).toBe('big bad');
    expect(screen.getByText('offline').className).toBe('badge fail');
    expect(screen.getByText(/No relay is connected through it. Checked/)).toBeTruthy();
  });

  it('warns when there is no proxy', async () => {
    setup();
    expect(await screen.findByText('Relays connect straight to this machine and can see its IP address.')).toBeTruthy();
    expect(screen.getByText('none').className).toBe('badge to_fix');
  });

  it('lists the recent drops as a table that leads to all invoices', async () => {
    const rows = [adminInvoice({ index: 0x7f3a, status: 'detected' }), adminInvoice({ id: 'e'.repeat(32), index: 1 })];
    const t = setup({ listInvoices: vi.fn().mockResolvedValue({ invoices: rows, next_before: null }) });
    const drops = await screen.findByRole('region', { name: 'Recent drops' });
    const cells = [...drops.querySelectorAll('tbody tr')].map((r) => r.querySelector('td')!.textContent);
    expect(cells).toEqual(['7F3A', '0001']);
    expect(drops.textContent).toContain(rows[0].address);
    await fireEvent.click(screen.getByRole('button', { name: 'All invoices' }));
    expect(t.onDrops).toHaveBeenCalled();
  });

  it('leaves out drops it cannot read', async () => {
    setup({ listInvoices: vi.fn().mockRejectedValue(new Error('unauthorized')) });
    await screen.findByText('Running');
    expect(screen.queryByRole('region', { name: 'Recent drops' })).toBeNull();
  });
});
