import { describe, expect, it, vi } from 'vitest';
import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import Maintenance from './Maintenance.svelte';
import type { PayApi } from '../lib/api';
import { audit, health } from '../test-helpers';

function setup(over: Record<string, ReturnType<typeof vi.fn>> = {}, answer = true) {
  const api = {
    getHealth: vi.fn().mockResolvedValue(health()),
    getAuditLog: vi.fn().mockResolvedValue(audit()),
    control: vi.fn().mockResolvedValue({ paused: true, restart_pending: false }),
    backup: vi.fn().mockResolvedValue({ file: 'deadhop-001700000000-000.sqlite', size_bytes: 4096, created_at: 1_700_000_000 }),
    listBackups: vi.fn().mockResolvedValue({ backups: [] }),
    wipe: vi.fn().mockResolvedValue({ deleted: 3, backup: { file: 'deadhop-001700000000-001.sqlite', size_bytes: 1, created_at: 1 } }),
    ...over,
  };
  const confirm = vi.fn().mockReturnValue(answer);
  render(Maintenance, { props: { api: api as unknown as PayApi, confirm } });
  return { ...api, confirm };
}

describe('Maintenance', () => {
  it('asks the browser to confirm by default', async () => {
    const ask = vi.spyOn(window, 'confirm').mockReturnValue(false);
    const api = {
      getHealth: vi.fn().mockResolvedValue(health()),
      getAuditLog: vi.fn().mockResolvedValue(audit()),
      listBackups: vi.fn().mockResolvedValue({ backups: [] }),
      control: vi.fn(),
    };
    render(Maintenance, { props: { api: api as unknown as PayApi } });
    await fireEvent.click(await screen.findByRole('button', { name: 'Stop invoicing' }));
    expect(ask).toHaveBeenCalled();
    expect(api.control).not.toHaveBeenCalled();
    ask.mockRestore();
  });

  it('stops, restarts and backs up', async () => {
    const t = setup();
    await screen.findByText('Invoicing is running.');
    expect(screen.getByText('No backup yet.')).toBeTruthy();
    await fireEvent.click(screen.getByRole('button', { name: 'Stop invoicing' }));
    await waitFor(() => expect(t.control).toHaveBeenCalledWith('stop'));
    expect(await screen.findByText(/Invoicing stopped. Payments/)).toBeTruthy();
    await fireEvent.click(screen.getByRole('button', { name: 'Restart relay connections' }));
    await waitFor(() => expect(t.control).toHaveBeenCalledWith('restart'));
    await screen.findByText(/Reconnecting to the relays/);
    await fireEvent.click(screen.getByRole('button', { name: 'Back up database' }));
    await screen.findByText('Backup written on the service host: backups/deadhop-001700000000-000.sqlite (4.0 KB).');
    expect(t.getHealth.mock.calls.length).toBeGreaterThanOrEqual(4);
    expect(t.listBackups.mock.calls.length).toBe(t.getHealth.mock.calls.length);
    expect(screen.queryByRole('list', { name: 'Backups on the host' })).toBeNull();
  });

  it('lists the backups on the host, newest first', async () => {
    const listBackups = vi
      .fn()
      .mockResolvedValueOnce({
        backups: [
          { file: 'deadhop-001700000060-000.sqlite', size_bytes: 2_202_009, created_at: 1_700_000_060 },
          { file: 'deadhop-001700000000-000.sqlite', size_bytes: 4096, created_at: 1_700_000_000 },
        ],
      })
      .mockRejectedValueOnce(new Error('Could not reach the service.'))
      .mockResolvedValue({ backups: [] });
    setup({ listBackups });
    const list = await screen.findByRole('list', { name: 'Backups on the host' });
    const rows = [...list.querySelectorAll('li')].map((li) => [...li.querySelectorAll('span')].map((s) => s.textContent));
    expect(rows).toEqual([
      ['deadhop-001700000060-000.sqlite', '2.1 MB', '2023-11-14 22:14 UTC'],
      ['deadhop-001700000000-000.sqlite', '4.0 KB', '2023-11-14 22:13 UTC'],
    ]);
    // A failed listing says so in the card and keeps the rest of the page.
    await fireEvent.click(screen.getByRole('button', { name: 'Back up database' }));
    await screen.findByText('Could not list the backups: Could not reach the service.');
    expect(screen.queryByRole('alert')).toBeNull();
    // The next good listing clears the message.
    await fireEvent.click(screen.getByRole('button', { name: 'Back up database' }));
    await waitFor(() => expect(screen.queryByText(/Could not list the backups/)).toBeNull());
    expect(screen.queryByRole('list', { name: 'Backups on the host' })).toBeNull();
  });

  it('starts a stopped service, and does not stop without confirmation', async () => {
    const paused = health({ service: { ...health().service, paused: true, restart_pending: true }, database: { ...health().database, last_backup_at: 1_700_000_000, backups: 2 } });
    const t = setup({ getHealth: vi.fn().mockResolvedValue(paused) });
    await screen.findByText('Invoicing is stopped.');
    expect(screen.getByText(/Restarting…/)).toBeTruthy();
    expect(screen.getByText('Last backup 2023-11-14 22:13 UTC; 2 kept.')).toBeTruthy();
    await fireEvent.click(screen.getByRole('button', { name: 'Start invoicing' }));
    await waitFor(() => expect(t.control).toHaveBeenCalledWith('start'));
    await screen.findByText('Invoicing started.');

    document.body.innerHTML = '';
    const u = setup({}, false);
    await fireEvent.click(await screen.findByRole('button', { name: 'Stop invoicing' }));
    expect(u.control).not.toHaveBeenCalled();
  });

  it('wipes only after typing WIPE, with the chosen options', async () => {
    const t = setup();
    const wipe = (await screen.findByRole('button', { name: 'Wipe' })) as HTMLButtonElement;
    expect(wipe.disabled).toBe(true);
    await fireEvent.input(screen.getByLabelText('Type WIPE to confirm'), { target: { value: 'WIPE' } });
    await fireEvent.click(screen.getByLabelText('Also forget settings changed here'));
    await fireEvent.click(screen.getByLabelText('Wipe even if customers may still pay open invoices'));
    await fireEvent.click(wipe);
    await waitFor(() => expect(t.wipe).toHaveBeenCalledWith({ confirm: 'WIPE', include_settings: true, force: true }));
    await screen.findByText('Deleted 3 invoices. A backup was taken first: backups/deadhop-001700000000-001.sqlite.');
    expect((screen.getByLabelText('Type WIPE to confirm') as HTMLInputElement).value).toBe('');

    document.body.innerHTML = '';
    setup({ wipe: vi.fn().mockResolvedValue({ deleted: 1, backup: { file: 'deadhop-001700000000-002.sqlite', size_bytes: 1, created_at: 1 } }) });
    await fireEvent.input(await screen.findByLabelText('Type WIPE to confirm'), { target: { value: 'WIPE' } });
    await fireEvent.click(screen.getByRole('button', { name: 'Wipe' }));
    await screen.findByText(/Deleted 1 invoice\./);
  });

  it('reports errors', async () => {
    setup({ backup: vi.fn().mockRejectedValue(new Error('The service had an internal error.')) });
    await screen.findByText('Wipe database');
    await fireEvent.click(screen.getByRole('button', { name: 'Back up database' }));
    await waitFor(() => expect(screen.getByRole('alert').textContent).toBe('The service had an internal error.'));

    document.body.innerHTML = '';
    setup({ getHealth: vi.fn().mockRejectedValue(new Error('Could not reach the service.')) });
    await waitFor(() => expect(screen.getByRole('alert').textContent).toBe('Could not reach the service.'));
    expect(screen.getByRole('button', { name: 'Stop invoicing' })).toBeTruthy();
  });
});
