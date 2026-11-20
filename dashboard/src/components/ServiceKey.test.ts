import { describe, expect, it, vi } from 'vitest';
import { fireEvent, render, screen } from '@testing-library/svelte';
import { nip19 } from 'nostr-tools';
import ServiceKey from './ServiceKey.svelte';
import { PUBKEY } from '../test-helpers';

const NPUB = nip19.npubEncode(PUBKEY);

describe('ServiceKey', () => {
  it('shows the whole key and copies it as npub or hex', async () => {
    const copy = vi.fn().mockResolvedValue(undefined);
    render(ServiceKey, { props: { pubkey: PUBKEY, relays: ['wss://a.example', 'wss://b.example'], copy } });
    expect(screen.getByRole('heading', { name: 'Service key' })).toBeTruthy();
    expect(screen.getByText(NPUB).tagName).toBe('CODE');
    expect(screen.getByText('Relays this page uses: wss://a.example, wss://b.example')).toBeTruthy();

    await fireEvent.click(screen.getByRole('button', { name: 'Copy npub' }));
    expect(copy).toHaveBeenLastCalledWith(NPUB);
    expect(screen.getByRole('button', { name: 'Copied' })).toBeTruthy();
    expect(screen.getByRole('button', { name: 'Copy hex' })).toBeTruthy();

    await fireEvent.click(screen.getByRole('button', { name: 'Copy hex' }));
    expect(copy).toHaveBeenLastCalledWith(PUBKEY);
    expect(screen.getByRole('button', { name: 'Copy npub' })).toBeTruthy();
  });

  it('says so when copying fails, and recovers on the next copy', async () => {
    const copy = vi.fn().mockRejectedValueOnce(new Error('denied')).mockResolvedValue(undefined);
    render(ServiceKey, { props: { pubkey: PUBKEY, copy } });
    expect(screen.queryByText(/Relays this page uses/)).toBeNull();
    await fireEvent.click(screen.getByRole('button', { name: 'Copy npub' }));
    expect(screen.getByRole('alert').textContent).toContain('copy it by hand');
    await fireEvent.click(screen.getByRole('button', { name: 'Copy npub' }));
    expect(screen.queryByRole('alert')).toBeNull();
  });

  it('has a one-line version for the footer that uses the browser clipboard by default', async () => {
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.defineProperty(navigator, 'clipboard', { value: { writeText }, configurable: true });
    render(ServiceKey, { props: { pubkey: PUBKEY, compact: true } });
    expect(screen.queryByRole('heading')).toBeNull();
    expect(screen.getByText(NPUB)).toBeTruthy();
    await fireEvent.click(screen.getByRole('button', { name: 'Copy' }));
    expect(writeText).toHaveBeenCalledWith(NPUB);
    expect(screen.getByRole('button', { name: 'Copied' })).toBeTruthy();
  });

  it('reports a failed copy in the footer too', async () => {
    render(ServiceKey, { props: { pubkey: PUBKEY, compact: true, copy: vi.fn().mockRejectedValue(new Error('no')) } });
    await fireEvent.click(screen.getByRole('button', { name: 'Copy' }));
    expect(screen.getByRole('alert')).toBeTruthy();
  });
});
