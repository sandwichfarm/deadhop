import { describe, expect, it } from 'vitest';
import { render, screen } from '@testing-library/svelte';
import ProxyStatus from './ProxyStatus.svelte';
import { proxyCheck } from '../test-helpers';
import type { ProxyCheck } from '../lib/schemas';

const show = (props: { check: ProxyCheck | null; checking?: boolean; error?: string; showRelays?: boolean }) => render(ProxyStatus, { props });
const box = () => screen.getByRole('status');

describe('ProxyStatus', () => {
  it('can leave the per-relay lines to the page', () => {
    show({ check: proxyCheck(), showRelays: false });
    expect(screen.getByText('Connected')).toBeTruthy();
    expect(box().querySelector('ul')).toBeNull();
  });

  it('shows nothing before a check', () => {
    show({ check: null });
    expect(screen.queryByRole('status')).toBeNull();
  });

  it('shows a check in progress as pending, and a failed request', () => {
    const { unmount } = show({ check: proxyCheck(), checking: true });
    expect(box().className).toContain('pending');
    expect(screen.getByText('Checking…')).toBeTruthy();
    unmount();
    show({ check: null, error: 'rate limit exceeded' });
    expect(box().className).toContain('bad');
    expect(screen.getByText('rate limit exceeded')).toBeTruthy();
  });

  it('tells working, slow, partial, failed and direct apart', () => {
    const cases: [ProxyCheck, string, string][] = [
      [proxyCheck(), 'ok', 'Connected'],
      [proxyCheck({ relays: [{ relay: 'wss://a', ok: true, code: 'HTTP 200', reason: '', ms: null }] }), 'ok', 'Connected'],
      [proxyCheck({ relays: [{ relay: 'wss://a', ok: true, code: 'SOCKS5 00', reason: '', ms: 2800 }] }), 'warn', 'Connected, but slow'],
      [
        proxyCheck({
          status: 'partial',
          relays: [
            { relay: 'wss://a', ok: true, code: 'SOCKS5 00', reason: '', ms: null },
            { relay: 'wss://b', ok: false, code: 'SOCKS5 04', reason: 'The proxy could not reach the relay (host unreachable).', ms: null },
          ],
        }),
        'warn',
        '1 of 2 relays reachable',
      ],
      [proxyCheck({ status: 'failed', relays: [{ relay: 'wss://a', ok: false, code: 'HTTP 407', reason: 'Needs a login.', ms: null }] }), 'bad', 'Not working'],
      [proxyCheck({ status: 'direct', proxy: '', relays: [] }), 'warn', 'No proxy'],
    ];
    for (const [check, tone, title] of cases) {
      const { unmount } = show({ check });
      expect(box().className).toContain(tone);
      expect(screen.getByText(title)).toBeTruthy();
      unmount();
    }
    show({ check: cases[3][0] });
    expect(screen.getByText('SOCKS5 04')).toBeTruthy();
    expect(screen.getByText('The proxy could not reach the relay (host unreachable).')).toBeTruthy();
  });
});
