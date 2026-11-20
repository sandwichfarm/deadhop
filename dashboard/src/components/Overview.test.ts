import { describe, expect, it, vi } from 'vitest';
import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import Overview from './Overview.svelte';
import type { PayApi } from '../lib/api';
import { adminInvoice, health, metrics, privacyAudit, proxyCheck, series } from '../test-helpers';

function setup(over: Partial<Record<'getHealth' | 'getMetrics' | 'getSeries' | 'getPrivacyAudit' | 'listInvoices', ReturnType<typeof vi.fn>>> = {}, refreshMs = 60_000, onAudit = vi.fn()) {
  const api = {
    getHealth: vi.fn().mockResolvedValue(health()),
    getMetrics: vi.fn().mockResolvedValue(metrics()),
    getSeries: vi.fn().mockResolvedValue(series()),
    getPrivacyAudit: vi.fn().mockResolvedValue(privacyAudit()),
    ...over,
  };
  render(Overview, { props: { api: api as unknown as PayApi, refreshMs, onAudit } });
  return api;
}

describe('Overview', () => {
  it('shows health, totals, gated windows and the chart', async () => {
    const api = setup();
    await screen.findByText('Running');
    expect(screen.getByText('up 1h 1m, v0.1.0')).toBeTruthy();
    expect(screen.getByText('0 restarts')).toBeTruthy();
    expect(screen.getByText('1 of 1 connected')).toBeTruthy();
    expect(screen.getByText('direct connection')).toBeTruthy();
    expect(screen.getByText('block 101')).toBeTruthy();
    expect(screen.getByText('plain TCP')).toBeTruthy();
    expect(screen.getByText('Healthy')).toBeTruthy();
    expect(screen.getByText('2.0 KB, 10 invoices')).toBeTruthy();
    expect(screen.getByText('never backed up')).toBeTruthy();
    expect(screen.getByText('12')).toBeTruthy();
    expect(screen.getByText('0.00150000 BTC')).toBeTruthy();
    expect(screen.getByText('Last 24 hours')).toBeTruthy();
    expect(screen.getAllByText('3 so far')).toHaveLength(2);
    expect(screen.getByText('150,000 sats; complete after 30d 0h online')).toBeTruthy();
    expect(screen.getByText('150,000 sats; complete after 365d 0h online')).toBeTruthy();
    expect(screen.getByText('Off.')).toBeTruthy();
    expect(screen.getByRole('img').getAttribute('aria-label')).toContain('100,000 sats in 7 buckets');
    expect(api.getSeries).toHaveBeenCalledWith('week');

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
      getMetrics: vi.fn().mockResolvedValue(
        metrics({ sweep: { enabled: true, mode: 'coinjoin', threshold_sat: 100_000, received_sat: 150_000, due: true } }),
      ),
    });
    await screen.findByText('Invoicing stopped');
    expect(screen.getByText('1 restart, restarting…')).toBeTruthy();
    expect(screen.getByText('0 of 2 connected')).toBeTruthy();
    expect(screen.getByText('via proxy')).toBeTruthy();
    expect(screen.getByText('Unreachable')).toBeTruthy();
    expect(screen.getByText('block height not known yet')).toBeTruthy();
    expect(screen.getByText('not polled yet')).toBeTruthy();
    expect(screen.getByText('Integrity check failed')).toBeTruthy();
    expect(screen.getByText('backed up 2023-11-14 22:13 UTC')).toBeTruthy();
    expect(screen.getByText('2 invoices could not be read from the chain.')).toBeTruthy();
    expect(screen.getByText('The Electrum server is not on this machine.')).toBeTruthy();
    expect(screen.getByText('TLS, pinned certificate')).toBeTruthy();
    expect(screen.getByText('invoicing is stopped')).toBeTruthy();
    expect(screen.getByText(/CoinJoin above 0.00100000 BTC/)).toBeTruthy();
    expect(screen.getByText('The threshold is reached: a sweep is due.')).toBeTruthy();
  });

  it('handles a single unreadable invoice, a sweep below its threshold and errors', async () => {
    setup({
      getHealth: vi.fn().mockResolvedValue(health({ chain: { ok: true, tip_height: 5, last_poll_at: 1, poll_secs: 10, watch_errors: 1, backend_local: true, transport: 'tls' } })),
      getMetrics: vi.fn().mockResolvedValue(metrics({ sweep: { enabled: true, mode: 'payjoin', threshold_sat: 9_000_000, received_sat: 1, due: false } })),
    });
    await screen.findByText('1 invoice could not be read from the chain.');
    expect(screen.getByText('TLS (public authorities)')).toBeTruthy();
    expect(screen.getByText(/PayJoin above/)).toBeTruthy();
    expect(screen.getByText('Below the threshold.')).toBeTruthy();

    document.body.innerHTML = '';
    setup({ getHealth: vi.fn().mockRejectedValue(new Error('The service had an internal error.')) });
    await waitFor(() => expect(screen.getByRole('alert').textContent).toBe('The service had an internal error.'));
  });

  it('refreshes on a timer', async () => {
    const api = setup({}, 20);
    await waitFor(() => expect(api.getHealth.mock.calls.length).toBeGreaterThan(2));
  });

  it('shows the privacy score, which opens the audit', async () => {
    const onAudit = vi.fn();
    setup({}, 60_000, onAudit);
    const tile = await screen.findByRole('button', { name: 'Privacy score 90 out of 100. Open the privacy audit' });
    expect(tile.textContent).toContain('1 needs attention');
    expect(tile.textContent).toContain('8 of 10 checked');
    expect(screen.getByText('90 / 100').className).toBe('ok');
    await fireEvent.click(tile);
    expect(onAudit).toHaveBeenCalled();
  });

  it('colours the score and says when nothing needs fixing', async () => {
    setup({ getPrivacyAudit: vi.fn().mockResolvedValue(privacyAudit({ score: 70, checks: [] })) });
    expect((await screen.findByText('70 / 100')).className).toBe('warn');
    expect(screen.getByText('Nothing needs attention')).toBeTruthy();
  });

  it('shows a low score in red, and no tile when the audit fails', async () => {
    const two = privacyAudit().checks.map((c) => ({ ...c, result: 'fail' as const }));
    setup({ getPrivacyAudit: vi.fn().mockResolvedValue(privacyAudit({ score: 40, checks: two })) });
    expect((await screen.findByText('40 / 100')).className).toBe('bad');
    expect(screen.getByText('4 need attention')).toBeTruthy();
  });

  it('leaves the score out when the audit cannot be read', async () => {
    setup({ getPrivacyAudit: vi.fn().mockRejectedValue(new Error('unauthorized')) });
    await screen.findByText('Running');
    expect(screen.queryByText(/Privacy score/)).toBeNull();
  });

  it('shows the proxy and its last check', async () => {
    setup({ getHealth: vi.fn().mockResolvedValue(health({ proxy: 'socks5://127.0.0.1:9050', proxy_check: proxyCheck() })) });
    expect(await screen.findByText('Online')).toBeTruthy();
    expect(screen.getByText('1 of 1 relays through it')).toBeTruthy();
    expect(screen.getByRole('status').textContent).toContain('Connected');
  });

  it('says when no relay is connected through the proxy, or there is none', async () => {
    setup({
      getHealth: vi.fn().mockResolvedValue(health({ proxy: 'socks5://127.0.0.1:9050', relays: [{ url: 'ws://127.0.0.1:7777', status: 'connecting', connected: false }] })),
    });
    expect(await screen.findByText('Not connected')).toBeTruthy();
  });

  it('warns when there is no proxy', async () => {
    setup();
    expect(await screen.findByText("Relays see this machine's IP address.")).toBeTruthy();
  });

  it('heads the page, lists the relays and the recent drops', async () => {
    const rows = [adminInvoice({ index: 0x7f3a, status: 'detected' }), adminInvoice({ id: 'e'.repeat(32), index: 1 })];
    setup({ listInvoices: vi.fn().mockResolvedValue({ invoices: rows, next_before: null }) });
    expect(await screen.findByRole('heading', { name: 'Serving. Nothing to reach.' })).toBeTruthy();
    const relays = screen.getByRole('region', { name: 'Relay connections' });
    expect(relays.textContent).toContain('ws://127.0.0.1:7777');
    expect(relays.querySelector('.badge')!.className).toContain('confirmed');
    const drops = await screen.findByRole('region', { name: 'Recent drops' });
    expect(drops.textContent).toContain('7F3A');
    expect(drops.textContent).toContain('0001');
  });

  it('says when invoicing is stopped, shows dropped relays, and leaves out drops it cannot read', async () => {
    setup({
      getHealth: vi.fn().mockResolvedValue(
        health({ service: { ...health().service, paused: true }, relays: [{ url: 'wss://r', status: 'disconnected', connected: false }] }),
      ),
      listInvoices: vi.fn().mockRejectedValue(new Error('unauthorized')),
    });
    expect(await screen.findByRole('heading', { name: 'Invoicing stopped.' })).toBeTruthy();
    expect(screen.getByRole('region', { name: 'Relay connections' }).querySelector('.badge')!.className).toContain('expired');
    expect(screen.queryByRole('region', { name: 'Recent drops' })).toBeNull();
  });
});
