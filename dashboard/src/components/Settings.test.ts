import { describe, expect, it, vi } from 'vitest';
import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import Settings from './Settings.svelte';
import type { PayApi } from '../lib/api';
import { proxyCheck, settings, settingsView } from '../test-helpers';


function setup(view = settingsView(), over: Record<string, ReturnType<typeof vi.fn>> = {}, answer = true) {
  const api = {
    getSettings: vi.fn().mockResolvedValue(view),
    updateSettings: vi.fn().mockResolvedValue(view),
    resetSettings: vi.fn().mockResolvedValue(settingsView()),
    addOperator: vi.fn().mockResolvedValue(view),
    removeOperator: vi.fn().mockResolvedValue(view),
    checkProxy: vi.fn().mockResolvedValue(proxyCheck()),
    ...over,
  };
  const onChanged = vi.fn();
  const confirm = vi.fn().mockReturnValue(answer);
  render(Settings, { props: { api: api as unknown as PayApi, dashboardRelays: ['ws://127.0.0.1:7777'], onChanged, confirm } });
  return { ...api, onChanged, confirm };
}

const save = () => fireEvent.click(screen.getByRole('button', { name: 'Save changes' }));

describe('Settings', () => {
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
    expect(screen.getByText('Relays or proxy changed: saving reconnects the service.')).toBeTruthy();
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
    const api = { getSettings: vi.fn().mockResolvedValue(settingsView({ overridden: ['name'] })), resetSettings: vi.fn() };
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
    expect(screen.getByRole('alert').textContent).toContain('This page talks to the service through ws://127.0.0.1:7777');
    await save();
    expect(t.confirm).toHaveBeenCalledWith(expect.stringContaining('reconnect'));
    expect(t.updateSettings).not.toHaveBeenCalled();
  });

  it('caps the relay list at eight', async () => {
    const eight = Array.from({ length: 8 }, (_, i) => `wss://r${i}.example`);
    setup(settingsView({ settings: settings({ relays: eight }) }));
    await screen.findByText('wss://r7.example');
    expect((screen.getByRole('button', { name: 'Add' }) as HTMLButtonElement).disabled).toBe(true);
  });

  it('builds proxy settings and confirms the reconnect', async () => {
    const t = setup();
    await fireEvent.change(await screen.findByLabelText('Type'), { target: { value: 'socks5' } });
    await fireEvent.input(screen.getByLabelText('Host'), { target: { value: '127.0.0.1' } });
    await save();
    expect(screen.getByRole('alert').textContent).toContain('port');
    await fireEvent.input(screen.getByLabelText('Port'), { target: { value: '9050' } });
    await fireEvent.input(screen.getByLabelText('User name'), { target: { value: 'me' } });
    await fireEvent.input(screen.getByLabelText('Password'), { target: { value: 'pw' } });
    await save();
    await waitFor(() => expect(t.updateSettings).toHaveBeenCalledWith({ proxy: 'socks5://me:pw@127.0.0.1:9050' }));
    expect(t.confirm).toHaveBeenCalled();
  });

  it('keeps a saved proxy password unless it is re-entered, and shows a locked proxy', async () => {
    const withProxy = settingsView({ settings: settings({ proxy: 'socks5://me:***@127.0.0.1:9050' }) });
    const t = setup(withProxy);
    expect(((await screen.findByLabelText('Password')) as HTMLInputElement).placeholder).toBe('unchanged');
    await save();
    expect(screen.getByText('Nothing to save.')).toBeTruthy();
    await fireEvent.input(screen.getByLabelText('Port'), { target: { value: '9150' } });
    await save();
    expect(screen.getByRole('alert').textContent).toContain('Re-enter the proxy password');
    await fireEvent.change(screen.getByLabelText('Type'), { target: { value: 'none' } });
    await save();
    await waitFor(() => expect(t.updateSettings).toHaveBeenCalledWith({ proxy: '' }));

    document.body.innerHTML = '';
    setup(settingsView({ fixed: { ...settingsView().fixed, proxy_locked: true }, settings: settings({ proxy: 'socks5://127.0.0.1:9050' }) }));
    await screen.findByText(/Locked on the host/);
    expect(screen.getByText(/Current: socks5:\/\/127.0.0.1:9050/)).toBeTruthy();
    document.body.innerHTML = '';
    setup(settingsView({ fixed: { ...settingsView().fixed, proxy_locked: true } }));
    await screen.findByText(/Current: none/);
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

  describe('live proxy check', () => {
    it('checks the live proxy on open, then each new address once it stops changing', async () => {
      vi.useFakeTimers();
      try {
        const t = setup(settingsView({ settings: settings({ proxy: 'socks5://127.0.0.1:9050' }) }));
        await vi.advanceTimersByTimeAsync(800);
        expect(t.checkProxy).toHaveBeenCalledWith(undefined);
        expect(await screen.findByText('Connected')).toBeTruthy();
        expect(screen.getByText('SOCKS5 00 · 84 ms')).toBeTruthy();

        // An incomplete address is not checked.
        await fireEvent.input(screen.getByLabelText('Host'), { target: { value: '' } });
        await vi.advanceTimersByTimeAsync(800);
        expect(t.checkProxy).toHaveBeenCalledTimes(1);

        // A complete one is, once typing stops.
        t.checkProxy.mockResolvedValueOnce(
          proxyCheck({ status: 'failed', relays: [{ relay: 'ws://127.0.0.1:7777', ok: false, code: 'ECONNREFUSED', reason: 'Nothing is listening at 10.0.0.8:9050.', ms: null }] }),
        );
        await fireEvent.input(screen.getByLabelText('Host'), { target: { value: '10.0.0.8' } });
        await vi.advanceTimersByTimeAsync(400);
        expect(t.checkProxy).toHaveBeenCalledTimes(1);
        await vi.advanceTimersByTimeAsync(400);
        expect(t.checkProxy).toHaveBeenLastCalledWith('socks5://10.0.0.8:9050');
        expect(await screen.findByText('Not working')).toBeTruthy();
        expect(screen.getByText('Nothing is listening at 10.0.0.8:9050.')).toBeTruthy();

        // Test runs at once; a refused check says why.
        t.checkProxy.mockRejectedValueOnce(new Error('rate limit exceeded'));
        await fireEvent.click(screen.getByRole('button', { name: 'Test' }));
        expect(t.checkProxy).toHaveBeenLastCalledWith('socks5://10.0.0.8:9050');
        expect(await screen.findByText('rate limit exceeded')).toBeTruthy();
        // An incomplete address: Test checks the live proxy.
        await fireEvent.input(screen.getByLabelText('Host'), { target: { value: '' } });
        await fireEvent.click(screen.getByRole('button', { name: 'Test' }));
        expect(t.checkProxy).toHaveBeenLastCalledWith(undefined);
      } finally {
        vi.useRealTimers();
      }
    });

    it('tests only the live proxy when it is locked on the host', async () => {
      const view = settingsView();
      view.fixed.proxy_locked = true;
      const t = setup(view);
      await screen.findByText(/Locked on the host/);
      await fireEvent.click(screen.getByRole('button', { name: 'Test' }));
      expect(t.checkProxy).toHaveBeenLastCalledWith(undefined);
      expect(await screen.findByText('Connected')).toBeTruthy();
    });
  });
});
