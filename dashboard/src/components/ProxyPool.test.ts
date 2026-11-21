import { afterEach, describe, expect, it, vi } from 'vitest';
import { fireEvent, render, screen, waitFor, within } from '@testing-library/svelte';
import Settings from './Settings.svelte';
import type { PayApi } from '../lib/api';
import type { ProxyPool, ProxySettings } from '../lib/schemas';
import { activePool, health, proxyCheck, proxyPool, relayHistory, settings, settingsView } from '../test-helpers';

const entry = (id: string, url: string, name = '') => ({ id, name, url });
const TWO = [entry('a', 'socks5://me:***@10.0.0.1:1080', 'tor'), entry('b', 'http://10.0.0.2:3128')];

function setup(over: Partial<ProxySettings> = {}, pool: ProxyPool | null = null, locked = false) {
  const view = settingsView({ settings: settings({ proxies: { ...settings().proxies, ...over } }), fixed: { ...settingsView().fixed, proxy_locked: locked } });
  const fns = {
    getSettings: vi.fn().mockResolvedValue(view),
    updateSettings: vi.fn().mockResolvedValue(view),
    resetSettings: vi.fn(),
    checkProxy: vi.fn().mockResolvedValue(proxyCheck()),
    getRelayHistory: vi.fn().mockResolvedValue(relayHistory({ relays: [] })),
    getHealth: pool === null ? vi.fn().mockRejectedValue(new Error('no')) : vi.fn().mockResolvedValue(health({ proxy_pool: pool })),
  };
  render(Settings, { props: { api: fns as unknown as PayApi, dashboardRelays: ['ws://127.0.0.1:7777'], onChanged: vi.fn(), confirm: () => true } });
  return fns;
}

const save = () => fireEvent.click(screen.getByRole('button', { name: 'Save changes' }));
const names = () => screen.queryAllByLabelText(/^Drag to reorder proxy/).map((g) => g.closest('li')!.querySelector('.proxy-name')!.textContent);
const patchOf = (t: ReturnType<typeof setup>) => t.updateSettings.mock.calls.at(-1)![0];
const rows = () => [...document.querySelectorAll('li.proxy-row')];

afterEach(() => vi.useRealTimers());

describe('the proxy list', () => {
  it('starts empty, and creates a proxy that is saved without an id', async () => {
    const t = setup();
    await screen.findByText('No proxies. Relays connect straight to this machine.');
    await fireEvent.click(screen.getByRole('button', { name: 'Create proxy' }));
    await fireEvent.input(screen.getByLabelText('Label (optional)'), { target: { value: ' Tor ' } });
    await fireEvent.input(screen.getByLabelText('Port'), { target: { value: '' } });
    await fireEvent.click(screen.getByRole('button', { name: 'Add to list' }));
    expect(screen.getByRole('alert').textContent).toContain('port');
    await fireEvent.input(screen.getByLabelText('Port'), { target: { value: '9050' } });
    await fireEvent.input(screen.getByLabelText('User name'), { target: { value: 'me' } });
    await fireEvent.input(screen.getByLabelText('Password'), { target: { value: 'pw' } });
    await fireEvent.click(screen.getByRole('button', { name: 'Add to list' }));
    expect(screen.getByText('socks5://me:pw@127.0.0.1:9050')).toBeTruthy();
    expect(screen.getByText('not saved')).toBeTruthy();
    expect(screen.getByText('1 change')).toBeTruthy();
    await save();
    await waitFor(() => expect(t.updateSettings).toHaveBeenCalledWith({ proxies: { list: [{ name: 'Tor', url: 'socks5://me:pw@127.0.0.1:9050' }] } }));
  });

  it('refuses the same address twice, and can cancel', async () => {
    setup({ list: [entry('b', 'http://10.0.0.2:3128')] });
    await screen.findByText('http://10.0.0.2:3128');
    await fireEvent.click(screen.getByRole('button', { name: 'Create proxy' }));
    await fireEvent.change(screen.getByLabelText('Type'), { target: { value: 'http' } });
    await fireEvent.input(screen.getByLabelText('Host'), { target: { value: '10.0.0.2' } });
    await fireEvent.input(screen.getByLabelText('Port'), { target: { value: '3128' } });
    await fireEvent.click(screen.getByRole('button', { name: 'Add to list' }));
    expect(screen.getByRole('alert').textContent).toBe('That proxy is already listed.');
    await fireEvent.click(screen.getByRole('button', { name: 'Cancel' }));
    expect(screen.queryByLabelText('Host')).toBeNull();
  });

  it('caps the list at eight', async () => {
    setup({ list: Array.from({ length: 8 }, (_, i) => entry(`p${i}`, `socks5://10.0.0.${i + 1}:1080`)) });
    await screen.findByText('socks5://10.0.0.8:1080');
    expect((screen.getByRole('button', { name: 'Create proxy' }) as HTMLButtonElement).disabled).toBe(true);
    expect(screen.getByText('Eight proxies is the most.')).toBeTruthy();
  });

  it('moves a proxy with the arrow buttons, which stop at the ends, and sends the new order with ids', async () => {
    const t = setup({ list: TWO });
    await screen.findByText('tor');
    expect(names()).toEqual(['tor', 'Unnamed proxy']);
    expect((screen.getByRole('button', { name: 'Move proxy 1 up' }) as HTMLButtonElement).disabled).toBe(true);
    expect((screen.getByRole('button', { name: 'Move proxy 2 down' }) as HTMLButtonElement).disabled).toBe(true);
    await fireEvent.click(screen.getByRole('button', { name: 'Move proxy 2 up' }));
    expect(names()).toEqual(['Unnamed proxy', 'tor']);
    await save();
    // The hidden password travels as shown, with the id the service restores it by.
    await waitFor(() =>
      expect(patchOf(t)).toEqual({
        proxies: { list: [{ id: 'b', name: '', url: 'http://10.0.0.2:3128' }, { id: 'a', name: 'tor', url: 'socks5://me:***@10.0.0.1:1080' }] },
      }),
    );
    await screen.findByText('No unsaved changes.');
    await fireEvent.click(screen.getByRole('button', { name: 'Move proxy 1 down' }));
    expect(names()).toEqual(['Unnamed proxy', 'tor']);
    await fireEvent.click(screen.getByRole('button', { name: 'Discard' }));
    expect(names()).toEqual(['tor', 'Unnamed proxy']);
  });

  it('moves a proxy by dragging its handle, showing where it would land', async () => {
    const t = setup({ list: [...TWO, entry('c', 'socks5://10.0.0.3:9050', 'third')] });
    await screen.findByText('third');
    const grips = screen.getAllByLabelText(/^Drag to reorder proxy/);
    const set = vi.fn();
    await fireEvent.dragStart(grips[0], { dataTransfer: { setData: set } });
    expect(set).toHaveBeenCalledWith('text/plain', '0');
    expect(rows()[0].className).toContain('dragging');
    await fireEvent.dragOver(rows()[2]);
    expect(rows()[2].className).toContain('drop-target');
    await fireEvent.dragOver(rows()[0]);
    expect(rows()[0].className).not.toContain('drop-target');
    await fireEvent.dragOver(rows()[2]);
    await fireEvent.drop(rows()[2]);
    expect(names()).toEqual(['Unnamed proxy', 'third', 'tor']);
    expect(rows()[2].className).not.toContain('dragging');
    // A drop with nothing being dragged does nothing; a drag can be abandoned.
    await fireEvent.drop(rows()[0]);
    expect(names()).toEqual(['Unnamed proxy', 'third', 'tor']);
    await fireEvent.dragStart(screen.getAllByLabelText(/^Drag to reorder proxy/)[1]);
    await fireEvent.dragEnd(screen.getAllByLabelText(/^Drag to reorder proxy/)[1]);
    expect(rows()[1].className).not.toContain('dragging');
    await save();
    await waitFor(() => expect(patchOf(t).proxies.list.map((e: { id: string }) => e.id)).toEqual(['b', 'c', 'a']));
  });

  it('moves a proxy with the arrow keys on its handle', async () => {
    setup({ list: TWO });
    await screen.findByText('tor');
    await fireEvent.keyDown(screen.getByLabelText('Drag to reorder proxy 1'), { key: 'ArrowDown' });
    expect(names()).toEqual(['Unnamed proxy', 'tor']);
    await fireEvent.keyDown(screen.getByLabelText('Drag to reorder proxy 2'), { key: 'ArrowUp' });
    expect(names()).toEqual(['tor', 'Unnamed proxy']);
    await fireEvent.keyDown(screen.getByLabelText('Drag to reorder proxy 1'), { key: 'ArrowUp' });
    await fireEvent.keyDown(screen.getByLabelText('Drag to reorder proxy 1'), { key: 'Tab' });
    expect(names()).toEqual(['tor', 'Unnamed proxy']);
  });

  it('keeps a hidden password through an edit of the label, and asks for it when the address changes', async () => {
    const t = setup({ list: TWO });
    await screen.findByText('tor');
    await fireEvent.click(screen.getByRole('button', { name: 'Edit proxy 1' }));
    expect((screen.getByLabelText('Password') as HTMLInputElement).placeholder).toBe('unchanged');
    await fireEvent.input(screen.getByLabelText('Label (optional)'), { target: { value: 'onion' } });
    await fireEvent.click(screen.getByRole('button', { name: 'Update proxy' }));
    await save();
    await waitFor(() => expect(patchOf(t).proxies.list[0]).toEqual({ id: 'a', name: 'onion', url: 'socks5://me:***@10.0.0.1:1080' }));

    await fireEvent.click(screen.getByRole('button', { name: 'Edit proxy 1' }));
    await fireEvent.input(screen.getByLabelText('Port'), { target: { value: '9150' } });
    await fireEvent.click(screen.getByRole('button', { name: 'Update proxy' }));
    expect(screen.getByRole('alert').textContent).toContain('Re-enter the proxy password');
    await fireEvent.input(screen.getByLabelText('Password'), { target: { value: 'new' } });
    await fireEvent.click(screen.getByRole('button', { name: 'Update proxy' }));
    await save();
    await waitFor(() => expect(patchOf(t).proxies.list[0]).toEqual({ id: 'a', name: 'tor', url: 'socks5://me:new@10.0.0.1:9150' }));
  });

  it('removes a proxy', async () => {
    const t = setup({ list: TWO });
    await screen.findByText('tor');
    await fireEvent.click(screen.getByRole('button', { name: 'Edit proxy 2' }));
    await fireEvent.click(screen.getByRole('button', { name: 'Remove proxy 1' }));
    expect(screen.queryByLabelText('Host')).toBeNull();
    expect(names()).toEqual(['Unnamed proxy']);
    await save();
    await waitFor(() => expect(patchOf(t)).toEqual({ proxies: { list: [{ id: 'b', name: '', url: 'http://10.0.0.2:3128' }] } }));
  });

  it('shows how each proxy is doing, and which one is in use', async () => {
    const base = activePool().proxies[0];
    const pool = activePool({
      total: 3,
      proxies: [
        { ...base, id: 'a', name: 'tor' },
        { ...base, id: 'b', reachable: false, status: 'failed', reason: 'Nothing is listening at 10.0.0.2:3128.' },
        { ...base, id: 'c', reachable: null, checked_at: null, status: null },
      ],
      active: 'a',
    });
    setup({ list: [...TWO, entry('c', 'socks5://10.0.0.3:1')] }, pool);
    await screen.findByText('in use');
    expect(screen.getByText('reachable').className).toBe('badge pass');
    expect(screen.getByText('unreachable').className).toBe('badge fail');
    expect(screen.getByText('Nothing is listening at 10.0.0.2:3128.').tagName).toBe('P');
    expect(screen.getByText('not checked')).toBeTruthy();
    // A proxy the service has not heard of has no status yet.
    document.body.innerHTML = '';
    setup({ list: TWO }, proxyPool());
    expect((await screen.findAllByText('not checked')).length).toBe(2);
    // A failure with no reason given still shows as unreachable.
    document.body.innerHTML = '';
    setup({ list: TWO }, activePool({ active: 'a', proxies: [{ ...base, id: 'a', reachable: false, reason: null }] }));
    expect((await screen.findByText('unreachable')).getAttribute('title')).toBe('');
  });

  it('tests a saved proxy by id, and a changed or new one by its address', async () => {
    const t = setup({ list: TWO });
    await screen.findByText('tor');
    await fireEvent.click(screen.getByRole('button', { name: 'Test proxy 1' }));
    expect(t.checkProxy).toHaveBeenLastCalledWith({ id: 'a' });
    expect(await screen.findByText('Connected')).toBeTruthy();
    t.checkProxy.mockRejectedValueOnce(new Error('rate limit exceeded'));
    await fireEvent.click(screen.getByRole('button', { name: 'Test proxy 2' }));
    expect(await screen.findByText('rate limit exceeded')).toBeTruthy();

    await fireEvent.click(screen.getByRole('button', { name: 'Create proxy' }));
    await fireEvent.input(screen.getByLabelText('Host'), { target: { value: '10.9.9.9' } });
    await fireEvent.click(screen.getByRole('button', { name: 'Add to list' }));
    await fireEvent.click(screen.getByRole('button', { name: 'Test proxy 3' }));
    expect(t.checkProxy).toHaveBeenLastCalledWith('socks5://10.9.9.9:9050');
  });
});

describe('the strategy', () => {
  it('describes each mode and shows the interval fields that belong to it', async () => {
    const t = setup({ list: TWO });
    await screen.findByText('tor');
    expect(screen.getByText(/Use the first proxy in the list/)).toBeTruthy();
    expect(screen.queryByLabelText('Change proxy every')).toBeNull();

    await fireEvent.click(screen.getByLabelText('Round-robin'));
    expect(screen.getByText(/not per connection/)).toBeTruthy();
    expect((screen.getByLabelText('Change proxy every') as HTMLInputElement).value).toBe('1');
    expect(screen.queryByLabelText('Shortest time before a change')).toBeNull();
    await fireEvent.input(screen.getByLabelText('Change proxy every'), { target: { value: '30' } });
    await fireEvent.change(screen.getByLabelText('Change proxy every: unit'), { target: { value: 'm' } });
    await save();
    await waitFor(() => expect(t.updateSettings).toHaveBeenLastCalledWith({ proxies: { mode: 'round_robin', rotate_secs: 1800 } }));

    await fireEvent.click(screen.getByLabelText('Failover'));
    expect(screen.getByText(/returns to an earlier proxy once it has stayed up for 2 minutes/)).toBeTruthy();
    await fireEvent.click(screen.getByLabelText('Chaos'));
    expect((screen.getByLabelText('Shortest time before a change') as HTMLInputElement).value).toBe('5');
    expect((screen.getByLabelText('Longest time before a change') as HTMLInputElement).value).toBe('15');
    expect(screen.getByText('A random time in between, now 5m to 15m.')).toBeTruthy();
    await fireEvent.input(screen.getByLabelText('Longest time before a change'), { target: { value: '30' } });
    expect(screen.getByText('A random time in between, now 5m to 30m.')).toBeTruthy();
    expect(screen.getAllByText(/Rotation needs two reachable proxies/).length).toBeGreaterThan(0);
  });

  it('refuses a chaos range that runs backwards, an empty interval and one out of range', async () => {
    setup({ list: TWO, mode: 'chaos' });
    await screen.findByText('tor');
    await fireEvent.input(screen.getByLabelText('Shortest time before a change'), { target: { value: '20' } });
    expect(screen.getByText('The shortest chaos interval is longer than the longest.')).toBeTruthy();
    await fireEvent.input(screen.getByLabelText('Shortest time before a change'), { target: { value: '' } });
    expect(screen.getByText('Fill in every interval.')).toBeTruthy();
    await fireEvent.input(screen.getByLabelText('Shortest time before a change'), { target: { value: '10' } });
    await fireEvent.change(screen.getByLabelText('Shortest time before a change: unit'), { target: { value: 's' } });
    expect(screen.getByText('Intervals run from 1 minute to 7 days.')).toBeTruthy();
    await fireEvent.input(screen.getByLabelText('Shortest time before a change'), { target: { value: '5' } });
    await fireEvent.change(screen.getByLabelText('Shortest time before a change: unit'), { target: { value: 'm' } });
    expect(screen.queryByText('Intervals run from 1 minute to 7 days.')).toBeNull();
  });

  it('says what the service is doing now', async () => {
    vi.useFakeTimers({ toFake: ['Date'] });
    vi.setSystemTime(1_700_000_000_000);
    const one = { ...activePool().proxies[0], id: 'a' };
    const get = async (pool: ProxyPool) => {
      document.body.innerHTML = '';
      setup({ list: TWO }, pool);
      await screen.findByText('tor');
      return (await screen.findByRole('status', { name: '' }).catch(() => null), await waitFor(() => document.querySelector('.strategy-state')!.textContent));
    };
    expect(await get(activePool({ total: 2, active: 'b', next_switch_at: 1_700_000_720, proxies: [one, { ...one, id: 'b' }] }))).toBe('Active · proxy 2 of 2 · next switch in 12m');
    expect(await get(activePool({ total: 2, active: 'a', proxies: [one] }))).toBe('Active · proxy 1 of 2');
    expect(await get(proxyPool({ state: 'suspended', reachable: 1, total: 2, active: 'a', proxies: [one] }))).toBe(
      'Suspended: rotation needs 2 reachable proxies, 1 is. That proxy keeps carrying traffic.',
    );
    expect(await get(proxyPool({ state: 'offline', offline: 'kill_switch', total: 2 }))).toBe('Offline: no proxy is reachable · kill switch');
    expect(await get(proxyPool({ state: 'direct' }))).toBe('No proxy is listed: relays connect straight.');
    expect(await get(proxyPool({ state: 'transparent', transparent: true }))).toBe('Transparent proxy declared: nothing is checked.');
  });
});

describe('the status of the proxies', () => {
  it('is read again every ten seconds', async () => {
    vi.useFakeTimers({ toFake: ['setInterval', 'clearInterval'] });
    const t = setup({ list: TWO });
    await screen.findByText('tor');
    expect(t.getHealth).toHaveBeenCalledTimes(1);
    await vi.advanceTimersByTimeAsync(10_000);
    expect(t.getHealth).toHaveBeenCalledTimes(2);
  });
});

describe('when no proxy is reachable', () => {
  it('offers four ways, with the kill switch as the default, and warns about the ones that go clearnet', async () => {
    const t = setup({ list: TWO });
    await screen.findByText('tor');
    const group = screen.getByRole('radiogroup', { name: 'When no proxy is reachable' });
    expect(within(group).getAllByRole('radio')).toHaveLength(4);
    expect((within(group).getByLabelText(/Kill switch/) as HTMLInputElement).checked).toBe(true);
    expect(within(group).getAllByText("Relays see this machine's IP address.")).toHaveLength(3);
    await fireEvent.click(within(group).getByLabelText(/Operators only/));
    expect(screen.getByText('1 change')).toBeTruthy();
    await fireEvent.click(within(group).getByLabelText(/Kill switch/));
    expect(screen.queryByText('1 change')).toBeNull();
    await fireEvent.click(within(group).getByLabelText(/Operators only/));
    await save();
    await waitFor(() => expect(t.updateSettings).toHaveBeenLastCalledWith({ proxies: { offline: 'admin_only' } }));
  });
});

describe('a transparent proxy', () => {
  it('is declared with one switch, which greys out everything else and keeps the data', async () => {
    const t = setup({ list: TWO });
    await screen.findByText('tor');
    const body = document.querySelector('fieldset.pool-body') as HTMLFieldSetElement;
    expect(body.disabled).toBe(false);
    await fireEvent.click(screen.getByLabelText(/I run a transparent proxy around this service/));
    expect(body.disabled).toBe(true);
    expect(body.className).toContain('greyed');
    expect(screen.getByText(/The service trusts it and checks nothing/)).toBeTruthy();
    expect(names()).toEqual(['tor', 'Unnamed proxy']);
    await save();
    await waitFor(() => expect(t.updateSettings).toHaveBeenLastCalledWith({ proxies: { transparent: true } }));
  });
});

describe('when the proxies are locked on the host', () => {
  it('disables every control and says why', async () => {
    setup({ list: TWO }, null, true);
    await screen.findByText(/Locked on the host/);
    expect((document.querySelector('fieldset.pool-body') as HTMLFieldSetElement).disabled).toBe(true);
    expect((screen.getByLabelText(/I run a transparent proxy/) as HTMLInputElement).disabled).toBe(true);
  });
});
