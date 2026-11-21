import { describe, expect, it, vi } from 'vitest';
import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import Connectivity from './Connectivity.svelte';
import type { PayApi } from '../lib/api';
import { health, relayHistory, relayStats } from '../test-helpers';
import type { RelayHealth } from '../lib/schemas';

const check = (relay: string, over: Partial<RelayHealth>): RelayHealth => ({
  relay, status: 'ok', connects: true, forwards: true, rate_limited: false, ms: 84, detail: '', checked_at: 1_700_000_000, ...over,
});

function setup(over: Record<string, ReturnType<typeof vi.fn>> = {}) {
  const api = {
    getRelayHistory: vi.fn().mockResolvedValue(relayHistory()),
    getHealth: vi.fn().mockResolvedValue(health()),
    checkRelays: vi.fn().mockResolvedValue({ relays: [] }),
    ...over,
  };
  const onBack = vi.fn();
  render(Connectivity, { props: { api: api as unknown as PayApi, refreshMs: 60_000, onBack } as never });
  return { ...api, onBack };
}

describe('Connectivity', () => {
  it('shows each relay with its error rate, counts, round trip and bars', async () => {
    const t = setup();
    const card = await screen.findByRole('region', { name: 'ws://127.0.0.1:7777' });
    expect(screen.getByRole('heading', { level: 1 }).textContent).toBe('Relays');
    expect(card.querySelector('.big')!.textContent).toBe('14%');
    expect(card.querySelector('.big')!.className).toBe('big warn');
    expect(card.querySelector('.relay-stats')!.textContent).toBe('4 health checks (0 failed) · 10 replies (2 failed) · average 120 ms');
    expect(card.querySelector('svg.bars.tall')).not.toBeNull();
    expect(card.querySelector('.dot')!.className).toBe('dot ok');
    expect(t.getRelayHistory).toHaveBeenCalledWith('1h');
    await fireEvent.click(screen.getByRole('button', { name: '← Overview' }));
    expect(t.onBack).toHaveBeenCalled();
  });

  it('switches the window and reloads', async () => {
    const t = setup();
    await screen.findByRole('region', { name: 'ws://127.0.0.1:7777' });
    for (const [label, value] of [['15 min', '15m'], ['12 hours', '12h'], ['1 day', '1d'], ['1 week', '1w'], ['1 hour', '1h']] as const) {
      await fireEvent.click(screen.getByRole('button', { name: label }));
      await waitFor(() => expect(t.getRelayHistory).toHaveBeenLastCalledWith(value));
      expect(screen.getByRole('button', { name: label }).getAttribute('aria-pressed')).toBe('true');
    }
  });

  it('says how a degraded or down relay failed, and marks a relay that was removed', async () => {
    const relays = [
      { url: 'wss://slow.example', status: 'connected', connected: true },
      { url: 'wss://gone.example', status: 'disconnected', connected: false },
    ];
    const relay_health = [
      check('wss://slow.example/', { status: 'degraded', forwards: false, ms: null, detail: 'The relay did not pass the test event on.' }),
      check('wss://gone.example', { status: 'down', connects: false, forwards: null, ms: null, detail: 'Could not connect to the relay.' }),
    ];
    setup({
      getHealth: vi.fn().mockResolvedValue(health({ relays, relay_health })),
      getRelayHistory: vi.fn().mockResolvedValue(
        relayHistory({
          relays: [
            relayStats({ relay: 'wss://slow.example', checks: 1, check_failures: 1, sends: 0, send_failures: 0, avg_ms: null }),
            relayStats({ relay: 'wss://gone.example', checks: 0, check_failures: 0, sends: 0, send_failures: 0, avg_ms: null }),
            relayStats({ relay: 'wss://old.example', checks: 1, check_failures: 0, sends: 1, send_failures: 0 }),
          ],
        }),
      ),
    });
    const slow = await screen.findByRole('region', { name: 'wss://slow.example' });
    expect(slow.querySelector('.badge')!.textContent).toBe('degraded');
    expect(slow.querySelector('.dot')!.className).toBe('dot warn');
    expect(slow.querySelector('.relay-detail')!.textContent).toBe('The relay did not pass the test event on.');
    expect(slow.querySelector('.relay-stats')!.textContent).toBe('1 health check (1 failed) · 0 replies (0 failed)');
    expect(slow.querySelector('.big')!.className).toBe('big bad');
    const gone = screen.getByRole('region', { name: 'wss://gone.example' });
    expect(gone.querySelector('.badge')!.textContent).toBe('down');
    expect(gone.querySelector('.dot')!.className).toBe('dot bad');
    expect(gone.querySelector('.big')!.textContent).toBe('—');
    const old = screen.getByRole('region', { name: 'wss://old.example' });
    expect(old.textContent).toContain('removed');
    expect(old.querySelector('.dot')!.className).toBe('dot bad');
    expect(old.querySelector('.relay-stats')!.textContent).toContain('1 reply (0 failed)');
  });

  it('checks again on demand and reports errors', async () => {
    const t = setup();
    await screen.findByRole('region', { name: 'ws://127.0.0.1:7777' });
    let finish: (v: unknown) => void = () => undefined;
    t.checkRelays.mockReturnValueOnce(new Promise((r) => (finish = r)));
    await fireEvent.click(screen.getByRole('button', { name: 'Check now' }));
    expect((screen.getByRole('button', { name: 'Checking…' }) as HTMLButtonElement).disabled).toBe(true);
    const before = t.getRelayHistory.mock.calls.length;
    finish({ relays: [] });
    await waitFor(() => expect(t.getRelayHistory.mock.calls.length).toBeGreaterThan(before));

    t.checkRelays.mockRejectedValueOnce(new Error('Too many requests; try again in a minute.'));
    await fireEvent.click(await screen.findByRole('button', { name: 'Check now' }));
    await waitFor(() => expect(screen.getByRole('alert').textContent).toBe('Too many requests; try again in a minute.'));
  });

  it('refreshes on a timer and shows a load error', async () => {
    const t = setup({ getRelayHistory: vi.fn().mockRejectedValue(new Error('Could not reach the service.')) });
    await waitFor(() => expect(screen.getByRole('alert').textContent).toBe('Could not reach the service.'));
    expect(screen.queryByRole('region')).toBeNull();
    expect(t.getHealth).toHaveBeenCalled();
  });

  it('reloads on a timer', async () => {
    const api = { getRelayHistory: vi.fn().mockResolvedValue(relayHistory()), getHealth: vi.fn().mockResolvedValue(health()), checkRelays: vi.fn() };
    render(Connectivity, { props: { api: api as unknown as PayApi, refreshMs: 20, onBack: vi.fn() } as never });
    await waitFor(() => expect(api.getRelayHistory.mock.calls.length).toBeGreaterThan(2));
  });
});
