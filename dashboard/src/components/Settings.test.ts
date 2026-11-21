import { describe, expect, it, vi } from 'vitest';
import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import Settings from './Settings.svelte';
import type { PayApi } from '../lib/api';
import { health, proxyCheck, relayHistory, relayStats, settings, settingsView } from '../test-helpers';


function setup(view = settingsView(), over: Record<string, ReturnType<typeof vi.fn>> = {}, answer = true) {
  const api = {
    getSettings: vi.fn().mockResolvedValue(view),
    updateSettings: vi.fn().mockResolvedValue(view),
    resetSettings: vi.fn().mockResolvedValue(settingsView()),
    addOperator: vi.fn().mockResolvedValue(view),
    removeOperator: vi.fn().mockResolvedValue(view),
    checkProxy: vi.fn().mockResolvedValue(proxyCheck()),
    getRelayHistory: vi.fn().mockResolvedValue(relayHistory({ relays: [] })),
    getHealth: vi.fn().mockResolvedValue(health()),
    ...over,
  };
  const onChanged = vi.fn();
  const confirm = vi.fn().mockReturnValue(answer);
  render(Settings, { props: { api: api as unknown as PayApi, dashboardRelays: ['ws://127.0.0.1:7777'], onChanged, confirm } });
  return { ...api, onChanged, confirm };
}

const save = () => fireEvent.click(screen.getByRole('button', { name: 'Save changes' }));

describe('Settings', () => {
  it("shows each relay's error rate for the last day, and leaves it out when it cannot be read", async () => {
    const history = relayHistory({ relays: [relayStats({ relay: 'ws://127.0.0.1:7777/', checks: 10, check_failures: 0, sends: 30, send_failures: 1 })] });
    setup(settingsView(), { getRelayHistory: vi.fn().mockResolvedValue(history) });
    expect((await screen.findByText('2.5% errors · 24 h')).className).toBe('rate-chip warn');

    document.body.innerHTML = '';
    setup(settingsView(), { getRelayHistory: vi.fn().mockRejectedValue(new Error('unauthorized')) });
    await screen.findByText('ws://127.0.0.1:7777');
    expect(screen.queryByText(/errors · 24 h/)).toBeNull();
  });

  it('counts unsaved changes in the save bar, says what saving does, and discards them', async () => {
    setup();
    await screen.findByText('No unsaved changes.');
    const discard = screen.getByRole('button', { name: 'Discard' }) as HTMLButtonElement;
    expect(discard.disabled).toBe(true);
    await fireEvent.input(screen.getByLabelText('Proof of work (bits)'), { target: { value: '18' } });
    expect(screen.getByText('1 change')).toBeTruthy();
    expect(screen.getByText('Saved as an override on top of the environment.')).toBeTruthy();
    await fireEvent.input(screen.getByLabelText('Add relay'), { target: { value: 'wss://new.example' } });
    await fireEvent.click(screen.getByRole('button', { name: 'Add' }));
    expect(screen.getByText('2 changes')).toBeTruthy();
    expect(screen.getByText('Relays or proxies changed: saving may reconnect the service.')).toBeTruthy();
    await fireEvent.input(screen.getByLabelText('Threshold (sats)'), { target: { value: '' } });
    expect(screen.getByText('Fill in every number.')).toBeTruthy();
    await fireEvent.input(screen.getByLabelText('Threshold (sats)'), { target: { value: '1000000' } });
    await fireEvent.click(discard);
    expect(screen.getByText('No unsaved changes.')).toBeTruthy();
    expect(screen.queryByText('wss://new.example')).toBeNull();
  });

  it('sends every field that was changed', async () => {
    const t = setup();
    const fields: [string, string][] = [
      ['Name', 'Shop'],
      ['Request age limit (s)', '120'],
      ['Public invoices / min', '10'],
      ['Public invoices / min / key', '2'],
      ['Public requests / min', '600'],
      ['Public requests / min / key', '60'],
      ['Confirmations', '3'],
      ['Smallest invoice (sats)', '2000'],
      ['Largest invoice (sats)', '5000000'],
      ['Default lifetime (s)', '600'],
      ['Longest lifetime (s)', '7200'],
      ['Open invoices cap', '50'],
      ['Invoices per day cap', '100'],
      ['Chain poll interval (s)', '30'],
      ['Late payment grace (s)', '3600'],
      ['Threshold (sats)', '200000'],
      ['Highest fee rate (sat/vB)', '5'],
      ['Smallest CoinJoin anonymity set', '10'],
    ];
    await screen.findByLabelText('Name');
    for (const [label, value] of fields) await fireEvent.input(screen.getByLabelText(label), { target: { value } });
    await save();
    await waitFor(() =>
      expect(t.updateSettings).toHaveBeenCalledWith({
        name: 'Shop',
        pow_window_secs: 120,
        rate_global_per_min: 10,
        rate_per_key_per_min: 2,
        rate_requests_per_min: 600,
        rate_requests_per_key_per_min: 60,
        min_confirmations: 3,
        min_amount_sat: 2000,
        max_amount_sat: 5_000_000,
        default_expiry_secs: 600,
        max_expiry_secs: 7200,
        max_open_invoices: 50,
        max_invoices_per_day: 100,
        poll_secs: 30,
        late_grace_secs: 3600,
        sweep: { threshold_sat: 200_000, max_fee_rate_sat_vb: 5, min_anonymity_set: 10 },
      }),
    );
  });

  it('asks the browser to confirm by default', async () => {
    const ask = vi.spyOn(window, 'confirm').mockReturnValue(false);
    const api = {
      getSettings: vi.fn().mockResolvedValue(settingsView({ overridden: ['name'] })),
      getRelayHistory: vi.fn().mockResolvedValue(relayHistory({ relays: [] })),
      getHealth: vi.fn().mockResolvedValue(health()),
      resetSettings: vi.fn(),
    };
    render(Settings, { props: { api: api as unknown as PayApi, dashboardRelays: [], onChanged: vi.fn() } });
    await fireEvent.click(await screen.findByRole('button', { name: 'Reset to environment' }));
    expect(ask).toHaveBeenCalled();
    expect(api.resetSettings).not.toHaveBeenCalled();
    ask.mockRestore();
  });

  it('sends only what changed', async () => {
    const t = setup();
    await fireEvent.input(await screen.findByLabelText('Proof of work (bits)'), { target: { value: '22' } });
    await fireEvent.click(screen.getByLabelText('Public invoicing'));
    await fireEvent.click(screen.getByLabelText('Enabled'));
    await fireEvent.change(screen.getByLabelText('Method'), { target: { value: 'coinjoin' } });
    await save();
    await waitFor(() =>
      expect(t.updateSettings).toHaveBeenCalledWith({ pow_bits: 22, public_invoicing: false, sweep: { enabled: true, mode: 'coinjoin' } }),
    );
    expect(t.confirm).not.toHaveBeenCalled();
    expect(t.onChanged).toHaveBeenCalled();
    expect(screen.getByText('Saved.')).toBeTruthy();
  });

  it('says when there is nothing to save and refuses empty numbers', async () => {
    const t = setup();
    await screen.findByLabelText('Proof of work (bits)');
    await save();
    expect(screen.getByText('Nothing to save.')).toBeTruthy();
    await fireEvent.input(screen.getByLabelText('Proof of work (bits)'), { target: { value: '' } });
    await save();
    expect(screen.getByRole('alert').textContent).toBe('Fill in every number.');
    await fireEvent.input(screen.getByLabelText('Proof of work (bits)'), { target: { value: '16' } });
    await fireEvent.input(screen.getByLabelText('Threshold (sats)'), { target: { value: '' } });
    await save();
    expect(screen.getByRole('alert').textContent).toBe('Fill in every number.');
    expect(t.updateSettings).not.toHaveBeenCalled();
  });

  it('adds and removes relays, warns about cutting this page off, and asks before reconnecting', async () => {
    const t = setup(settingsView(), {}, false);
    await screen.findByText('ws://127.0.0.1:7777');
    expect((screen.getByRole('button', { name: 'Remove ws://127.0.0.1:7777' }) as HTMLButtonElement).disabled).toBe(true);
    const add = screen.getByLabelText('Add relay');
    await fireEvent.input(add, { target: { value: 'https://nope' } });
    await fireEvent.click(screen.getByRole('button', { name: 'Add' }));
    expect(screen.getByRole('alert').textContent).toContain('starts with wss://');
    await fireEvent.input(add, { target: { value: 'ws://127.0.0.1:7777' } });
    await fireEvent.click(screen.getByRole('button', { name: 'Add' }));
    expect(screen.getByRole('alert').textContent).toContain('already listed');
    await fireEvent.input(add, { target: { value: 'wss://relay.example' } });
    await fireEvent.click(screen.getByRole('button', { name: 'Add' }));
    expect(screen.queryByRole('alert')).toBeNull();
    await fireEvent.click(screen.getByRole('button', { name: 'Remove ws://127.0.0.1:7777' }));
    expect(screen.getByRole('alert').textContent).toContain('This page signs in through the administration key, over ws://127.0.0.1:7777');
    await save();
    // Not a block: it asks, and the answer decides.
    expect(t.confirm).toHaveBeenCalledWith(expect.stringContaining('would no longer use any of them'));
    expect(t.updateSettings).not.toHaveBeenCalled();
  });

  it('asks twice when saving cuts this page off and reconnects, and saves when both are accepted', async () => {
    const t = setup(settingsView(), {}, true);
    await screen.findByText('ws://127.0.0.1:7777');
    await fireEvent.input(screen.getByLabelText('Add relay'), { target: { value: 'wss://relay.example' } });
    await fireEvent.click(screen.getByRole('button', { name: 'Add' }));
    await fireEvent.click(screen.getByRole('button', { name: 'Remove ws://127.0.0.1:7777' }));
    await save();
    await waitFor(() => expect(t.updateSettings).toHaveBeenCalledWith({ relays: ['wss://relay.example'] }));
    expect(t.confirm).toHaveBeenCalledTimes(2);
  });

  it('explains each key, with how many relays and proxies it uses', async () => {
    setup(settingsView({ settings: settings({ relays: ['ws://127.0.0.1:7777', 'wss://b.example'], relay_keys: { 'wss://b.example': ['invoices'] } }) }));
    const keys = await screen.findByRole('list', { name: 'Keys' });
    const lines = [...keys.querySelectorAll('li')].map((li) => li.textContent!.replace(/\s+/g, ' ').trim());
    expect(lines[0]).toContain('Public invoices');
    expect(lines[0]).toContain('2 relays · no proxies');
    expect(lines[1]).toContain('Administration');
    expect(lines[1]).toContain('Operators sign in here');
    expect(lines[1]).toContain('1 relay · no proxies');
  });

  it('counts the proxies each key uses', async () => {
    const proxies = { ...settings().proxies, list: [{ id: 'a', name: '', url: 'socks5://h:1', keys: ['invoices'] }, { id: 'b', name: '', url: 'socks5://h:2', keys: [] }] };
    setup(settingsView({ settings: settings({ proxies }) }));
    const keys = await screen.findByRole('list', { name: 'Keys' });
    expect([...keys.querySelectorAll('.count')].map((c) => c.textContent)).toEqual(['1 relay · 2 proxies', '1 relay · 1 proxy']);
  });

  describe('which key uses which relay', () => {
    const two = () => settingsView({ settings: settings({ relays: ['ws://127.0.0.1:7777', 'wss://b.example'] }) });
    const chip = (relay: string, key: string) => screen.getByLabelText(`${relay} used by ${key}`) as HTMLInputElement;

    it('starts with every key on every relay, and saves only the restricted ones', async () => {
      const t = setup(two());
      await screen.findByText('wss://b.example');
      expect(chip('wss://b.example', 'Administration').checked && chip('wss://b.example', 'Public invoices').checked).toBe(true);
      expect(screen.getAllByText('all keys')).toHaveLength(2);
      await fireEvent.click(chip('wss://b.example', 'Administration'));
      expect(screen.getByText('1 change')).toBeTruthy();
      expect(screen.getByText('Relays or proxies changed: saving may reconnect the service.')).toBeTruthy();
      expect(screen.getAllByText('all keys')).toHaveLength(1);
      await save();
      await waitFor(() => expect(t.updateSettings).toHaveBeenCalledWith({ relay_keys: { 'wss://b.example': ['invoices'] } }));
      expect(t.confirm).toHaveBeenCalledWith(expect.stringContaining('reconnect'));
    });

    it('does not save a restriction when the reconnect is not confirmed', async () => {
      const t = setup(two(), {}, false);
      await screen.findByText('wss://b.example');
      await fireEvent.click(chip('wss://b.example', 'Administration'));
      await save();
      expect(t.confirm).toHaveBeenCalledWith(expect.stringContaining('reconnect'));
      expect(t.updateSettings).not.toHaveBeenCalled();
    });

    it('does not let a relay lose its last key', async () => {
      setup(two());
      await screen.findByText('wss://b.example');
      await fireEvent.click(chip('wss://b.example', 'Administration'));
      expect(chip('wss://b.example', 'Public invoices').disabled).toBe(true);
    });

    it('going back to all keys is no change', async () => {
      setup(two());
      await screen.findByText('wss://b.example');
      await fireEvent.click(chip('wss://b.example', 'Administration'));
      await fireEvent.click(chip('wss://b.example', 'Administration'));
      expect(screen.getByText('No unsaved changes.')).toBeTruthy();
    });

    it('refuses, before asking the service, to leave a key with no relay', async () => {
      const t = setup(two());
      await screen.findByText('wss://b.example');
      await fireEvent.click(chip('wss://b.example', 'Administration'));
      await fireEvent.click(chip('ws://127.0.0.1:7777', 'Administration'));
      expect(screen.getByText('No relay is assigned to Administration: it could not be reached. Give it one.')).toBeTruthy();
      await save();
      expect(screen.getAllByRole('alert').map((a) => a.textContent).join(' ')).toContain('No relay is assigned to Administration');
      expect(t.updateSettings).not.toHaveBeenCalled();
    });

    it('lets an assignment that cannot be saved be discarded', async () => {
      setup(two());
      await screen.findByText('wss://b.example');
      await fireEvent.click(chip('wss://b.example', 'Administration'));
      await fireEvent.click(chip('ws://127.0.0.1:7777', 'Administration'));
      const discard = screen.getByRole('button', { name: 'Discard' }) as HTMLButtonElement;
      expect(discard.disabled).toBe(false);
      await fireEvent.click(discard);
      expect(screen.getByText('No unsaved changes.')).toBeTruthy();
    });

    it('warns, without blocking, when the administration key would lose this page', async () => {
      const t = setup(two());
      await screen.findByText('wss://b.example');
      await fireEvent.click(chip('ws://127.0.0.1:7777', 'Administration'));
      expect(screen.getByRole('alert').textContent).toContain('signs in through the administration key');
      await save();
      expect(t.confirm).toHaveBeenCalledWith(expect.stringContaining('administration key would no longer use'));
      await waitFor(() => expect(t.updateSettings).toHaveBeenCalledWith({ relay_keys: { 'ws://127.0.0.1:7777': ['invoices'] } }));
    });

    it('forgets a restriction when its relay is removed', async () => {
      const view = settingsView({ settings: settings({ relays: ['ws://127.0.0.1:7777', 'wss://b.example'], relay_keys: { 'wss://b.example': ['invoices'] } }) });
      const t = setup(view);
      await screen.findByText('wss://b.example');
      await fireEvent.click(screen.getByRole('button', { name: 'Remove wss://b.example' }));
      await save();
      await waitFor(() => expect(t.updateSettings).toHaveBeenCalledWith({ relays: ['ws://127.0.0.1:7777'], relay_keys: {} }));
    });

    it('refuses to leave a key with no proxy while there is a list', async () => {
      const proxies = { ...settings().proxies, list: [{ id: 'a', name: '', url: 'socks5://h:1', keys: [] }] };
      const t = setup(settingsView({ settings: settings({ proxies }) }));
      await fireEvent.click(await screen.findByLabelText('Proxy 1 used by Administration'));
      expect(screen.getByText('No proxy is assigned to Administration: it would connect without one. Give it a proxy.')).toBeTruthy();
      await save();
      expect(t.updateSettings).not.toHaveBeenCalled();
    });
  });

  it('caps the relay list at eight', async () => {
    const eight = Array.from({ length: 8 }, (_, i) => `wss://r${i}.example`);
    setup(settingsView({ settings: settings({ relays: eight }) }));
    await screen.findByText('wss://r7.example');
    expect((screen.getByRole('button', { name: 'Add' }) as HTMLButtonElement).disabled).toBe(true);
  });

  it('marks overridden values, shows warnings and resets', async () => {
    const view = settingsView({ overridden: ['name', 'pow_bits'], warnings: ['weak proof of work'], restart_pending: true });
    const t = setup(view);
    await screen.findByLabelText('Name *');
    expect(screen.getByLabelText('Proof of work (bits) *')).toBeTruthy();
    expect(screen.getByText('weak proof of work')).toBeTruthy();
    expect(screen.getByText('Reconnecting to the relays…')).toBeTruthy();
    await fireEvent.click(screen.getByRole('button', { name: 'Reset to environment' }));
    await waitFor(() => expect(t.resetSettings).toHaveBeenCalled());
    expect(screen.getByText('Back to the environment values.')).toBeTruthy();
  });

  it('does not reset without confirmation, and reports failures', async () => {
    const t = setup(settingsView({ overridden: ['name'] }), { updateSettings: vi.fn().mockRejectedValue(new Error('The service rejected the request as invalid.')) }, false);
    await fireEvent.click(await screen.findByRole('button', { name: 'Reset to environment' }));
    expect(t.resetSettings).not.toHaveBeenCalled();
    await fireEvent.input(screen.getByLabelText('Name *'), { target: { value: 'Shop' } });
    await save();
    await waitFor(() => expect(screen.getByRole('alert').textContent).toBe('The service rejected the request as invalid.'));

    document.body.innerHTML = '';
    setup(settingsView(), { getSettings: vi.fn().mockRejectedValue(new Error('This key is not allowed to do that.')) });
    await waitFor(() => expect(screen.getByRole('alert').textContent).toBe('This key is not allowed to do that.'));
  });

  it('shows what is fixed on the host', async () => {
    setup();
    expect(await screen.findByText('Fixed on the host')).toBeTruthy();
    expect(screen.getByText('127.0.0.1:50001')).toBeTruthy();
    expect(screen.getByText('off')).toBeTruthy();
  });

  it('shows discovery announcements when they are on', async () => {
    setup(settingsView({ fixed: { ...settingsView().fixed, announce: true } }));
    await screen.findByText('on');
  });
});
