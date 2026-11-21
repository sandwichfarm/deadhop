import { describe, expect, it, vi } from 'vitest';
import { fireEvent, render, screen, waitFor, within } from '@testing-library/svelte';
import { nip19 } from 'nostr-tools';
import Operators from './Operators.svelte';
import type { PayApi } from '../lib/api';
import { PUBKEY, audit, settingsView } from '../test-helpers';

const OTHER = 'c6047f9441ed7d6d3045406e95c07cd85c778e4b8cef3ca7abac09b95c709ee5';

function setup(view = settingsView(), over: Record<string, ReturnType<typeof vi.fn>> = {}, answer = true) {
  const api = {
    getSettings: vi.fn().mockResolvedValue(view),
    getAuditLog: vi.fn().mockResolvedValue(audit()),
    addOperator: vi.fn().mockResolvedValue(view),
    removeOperator: vi.fn().mockResolvedValue(view),
    ...over,
  };
  const confirm = vi.fn().mockReturnValue(answer);
  render(Operators, { props: { api: api as unknown as PayApi, confirm } });
  return { ...api, confirm };
}

describe('Operators', () => {
  it('adds operators by any key format and removes dashboard ones', async () => {
    const view = settingsView({
      operators: [
        { pubkey: PUBKEY, label: '', source: 'environment', added_at: null },
        { pubkey: OTHER, label: 'till 2', source: 'dashboard', added_at: 1_700_000_000 },
      ],
    });
    const t = setup(view);
    await screen.findByText('from DEADHOP_ADMINS');
    expect(screen.getByText('till 2')).toBeTruthy();
    await fireEvent.input(screen.getByLabelText('Public key'), { target: { value: 'npub1nope' } });
    await fireEvent.click(screen.getByRole('button', { name: 'Add operator' }));
    expect(screen.getByRole('alert').textContent).toContain('hex, npub or nprofile');
    const profile = nip19.nprofileEncode({ pubkey: OTHER, relays: ['wss://r.example'] });
    await fireEvent.input(screen.getByLabelText('Public key'), { target: { value: ` nostr:${profile} ` } });
    await fireEvent.input(screen.getByLabelText('Label'), { target: { value: ' till 3 ' } });
    await fireEvent.click(screen.getByRole('button', { name: 'Add operator' }));
    await waitFor(() => expect(t.addOperator).toHaveBeenCalledWith(`nostr:${profile}`, 'till 3'));
    expect(screen.getByText('Operator added.')).toBeTruthy();
    expect((screen.getByLabelText('Public key') as HTMLInputElement).value).toBe('');
    // The audit log is read again after a change.
    expect(t.getAuditLog).toHaveBeenCalledTimes(2);

    const row = screen.getByText('till 2').closest('tr')!;
    await fireEvent.click(within(row as HTMLElement).getByRole('button', { name: `Remove operator ${OTHER}` }));
    await waitFor(() => expect(t.removeOperator).toHaveBeenCalledWith(OTHER));
    expect(t.confirm).toHaveBeenCalledWith(expect.stringContaining('loses access'));
  });

  it('keeps an operator when removal is not confirmed, and reports refusals', async () => {
    const view = settingsView({ operators: [{ pubkey: OTHER, label: '', source: 'dashboard', added_at: 1 }] });
    const t = setup(view, {}, false);
    await fireEvent.click(await screen.findByRole('button', { name: `Remove operator ${OTHER}` }));
    expect(t.removeOperator).not.toHaveBeenCalled();

    document.body.innerHTML = '';
    setup(view, { removeOperator: vi.fn().mockRejectedValue(new Error('this is the last operator')) });
    await fireEvent.click(await screen.findByRole('button', { name: `Remove operator ${OTHER}` }));
    await waitFor(() => expect(screen.getByRole('alert').textContent).toBe('this is the last operator'));
  });

  it('shows the audit log, marking what the service did by itself', async () => {
    setup();
    await screen.findByText('update_settings');
    expect(screen.getByText('revert_connection').className).toContain('badge');
    expect(screen.getByText('update_settings').className).toBe('mono');
    expect(screen.getAllByText('79be667e…f81798').length).toBe(2);

    document.body.innerHTML = '';
    setup(settingsView(), { getAuditLog: vi.fn().mockResolvedValue({ entries: [] }) });
    await screen.findByText('No operator actions yet.');

    document.body.innerHTML = '';
    setup(settingsView(), { getSettings: vi.fn().mockRejectedValue(new Error('unauthorized')) });
    await waitFor(() => expect(screen.getByRole('alert').textContent).toBe('unauthorized'));
  });

  it('asks the browser to confirm by default', async () => {
    const ask = vi.spyOn(window, 'confirm').mockReturnValue(false);
    const view = settingsView({ operators: [{ pubkey: OTHER, label: '', source: 'dashboard', added_at: 1 }] });
    const api = { getSettings: vi.fn().mockResolvedValue(view), getAuditLog: vi.fn().mockResolvedValue(audit()), removeOperator: vi.fn() };
    render(Operators, { props: { api: api as unknown as PayApi } });
    await fireEvent.click(await screen.findByRole('button', { name: `Remove operator ${OTHER}` }));
    expect(ask).toHaveBeenCalled();
    expect(api.removeOperator).not.toHaveBeenCalled();
  });
});
