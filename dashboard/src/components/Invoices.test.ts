import { describe, expect, it, vi } from 'vitest';
import { fireEvent, render, screen, waitFor, within } from '@testing-library/svelte';
import Invoices from './Invoices.svelte';
import type { PayApi } from '../lib/api';
import { PUBKEY, adminInvoice, info, invoice, stats } from '../test-helpers';

const second = adminInvoice({ id: 'f'.repeat(32), index: 1, status: 'confirmed', received_sat: 50_000, memo: 'order 9' });

function setup(api: Partial<Record<keyof PayApi, ReturnType<typeof vi.fn>>>, refreshMs = 60_000) {
  const onCreated = vi.fn();
  const full = {
    getStats: vi.fn().mockResolvedValue(stats()),
    listInvoices: vi.fn().mockResolvedValue({ invoices: [second, adminInvoice()], next_before: null }),
    createInvoice: vi.fn().mockResolvedValue(invoice()),
    ...api,
  };
  const view = render(Invoices, { props: { api: full as unknown as PayApi, info: info({ is_admin: true }), onCreated, refreshMs } });
  return { ...full, onCreated, ...view };
}

describe('Invoices', () => {
  it('shows totals, chain health and the invoice table', async () => {
    const t = setup({});
    await waitFor(() => expect(screen.getByText('0.00150000 BTC')).toBeTruthy());
    expect(screen.getByText('3 paid')).toBeTruthy();
    expect(screen.getByText('0.00050000 BTC')).toBeTruthy();
    expect(screen.getByText('1 seen')).toBeTruthy();
    expect(screen.getByText('4 expired')).toBeTruthy();
    expect(screen.getByText('Connected')).toBeTruthy();
    expect(screen.getByText('block 101')).toBeTruthy();
    expect(t.listInvoices).toHaveBeenCalledWith({});

    const rows = screen.getAllByRole('row');
    expect(rows).toHaveLength(3);
    const first = within(rows[1]);
    await fireEvent.click(first.getByRole('button'));
    expect(screen.getByRole('link', { name: 'Open payment page' }).getAttribute('href')).toBe(`#/pay/${'f'.repeat(32)}`);
    expect(first.getByText('Paid')).toBeTruthy();
    expect(first.getByText('order 9')).toBeTruthy();
    expect(first.getByText('2023-11-14 22:13 UTC')).toBeTruthy();
    expect(first.getByTitle(PUBKEY).textContent).toBe('79be667e…f81798');
    expect(screen.queryByRole('button', { name: 'Load more' })).toBeNull();
  });

  it('reports an unreachable chain backend and an empty list', async () => {
    setup({
      getStats: vi.fn().mockResolvedValue(stats({ chain_ok: false, tip_height: null })),
      listInvoices: vi.fn().mockResolvedValue({ invoices: [], next_before: null }),
    });
    await waitFor(() => expect(screen.getByText('Unreachable')).toBeTruthy());
    expect(screen.getByText('block height not known yet')).toBeTruthy();
    expect(screen.getByText('No invoices yet.')).toBeTruthy();
  });

  it('flags invoices whose payments could not be read', async () => {
    setup({ getStats: vi.fn().mockResolvedValue(stats({ watch_errors: 1 })) });
    await waitFor(() => expect(screen.getByRole('alert').textContent).toContain('1 open invoice could not be read'));
    document.body.innerHTML = '';
    setup({ getStats: vi.fn().mockResolvedValue(stats({ watch_errors: 3 })) });
    await waitFor(() => expect(screen.getByRole('alert').textContent).toContain('3 open invoices could not be read'));
  });

  it('filters by status and pages through results', async () => {
    const listInvoices = vi
      .fn()
      .mockResolvedValueOnce({ invoices: [second], next_before: 1 })
      .mockResolvedValueOnce({ invoices: [adminInvoice()], next_before: null })
      .mockResolvedValueOnce({ invoices: [second], next_before: 1 })
      .mockResolvedValueOnce({ invoices: [], next_before: null });
    setup({ listInvoices });
    await fireEvent.click(await screen.findByRole('button', { name: 'Load more' }));
    await waitFor(() => expect(screen.getAllByRole('row')).toHaveLength(3));
    expect(listInvoices).toHaveBeenLastCalledWith({ before: 1 });
    expect(screen.queryByRole('button', { name: 'Load more' })).toBeNull();

    await fireEvent.click(screen.getByRole('button', { name: 'Paid' }));
    expect(screen.getByRole('button', { name: 'Paid' }).getAttribute('aria-pressed')).toBe('true');
    await waitFor(() => expect(listInvoices).toHaveBeenLastCalledWith({ status: 'confirmed' }));
    await fireEvent.click(await screen.findByRole('button', { name: 'Load more' }));
    await waitFor(() => expect(listInvoices).toHaveBeenLastCalledWith({ status: 'confirmed', before: 1 }));
  });

  it('refreshes on demand and on a timer, and stops when removed', async () => {
    const t = setup({}, 20);
    await waitFor(() => expect(t.getStats.mock.calls.length).toBeGreaterThanOrEqual(3));
    await fireEvent.click(screen.getByRole('button', { name: 'Refresh' }));
    t.unmount();
    const calls = t.getStats.mock.calls.length;
    await new Promise((r) => setTimeout(r, 60));
    expect(t.getStats.mock.calls.length).toBe(calls);
  });

  it('shows errors from loading and from paging', async () => {
    setup({ getStats: vi.fn().mockRejectedValue(new Error('This key is not allowed to do that.')) });
    await waitFor(() => expect(screen.getByRole('alert').textContent).toBe('This key is not allowed to do that.'));
    document.body.innerHTML = '';

    const listInvoices = vi
      .fn()
      .mockResolvedValueOnce({ invoices: [second], next_before: 1 })
      .mockRejectedValueOnce(new Error('Could not reach the service.'));
    setup({ listInvoices });
    await fireEvent.click(await screen.findByRole('button', { name: 'Load more' }));
    await waitFor(() => expect(screen.getByRole('alert').textContent).toBe('Could not reach the service.'));
  });

  it('lets the operator create an invoice', async () => {
    const t = setup({});
    await fireEvent.input(screen.getByLabelText('Amount (sats)'), { target: { value: '50000' } });
    await fireEvent.click(screen.getByRole('button', { name: 'Create invoice' }));
    await waitFor(() => expect(t.onCreated).toHaveBeenCalledWith(invoice()));
  });

  it('shows a drop with its progress, and closes it', async () => {
    const statuses = ['pending', 'detected', 'confirmed', 'expired'] as const;
    const rows = statuses.map((status, i) => adminInvoice({ id: `${i}`.repeat(32), index: 0x7f3a + i, status, memo: i === 0 ? 'notebook' : '' }));
    setup({ listInvoices: vi.fn().mockResolvedValue({ invoices: rows, next_before: null }) });
    const expected: Record<string, string[]> = {
      pending: ['done', 'transit', 'todo'],
      detected: ['done', 'done', 'transit'],
      confirmed: ['done', 'done', 'done'],
      expired: ['done', 'todo', 'todo'],
    };
    for (const [i, status] of statuses.entries()) {
      const id = (0x7f3a + i).toString(16).toUpperCase();
      await fireEvent.click(await screen.findByRole('button', { name: id }));
      const panel = screen.getByRole('complementary', { name: `Drop ${id}` });
      expect([...panel.querySelectorAll('ol.progress li')].map((li) => li.className)).toEqual(expected[status]);
    }
    await fireEvent.click(screen.getByRole('button', { name: '7F3A' }));
    expect(screen.getByText('Note: notebook')).toBeTruthy();
    expect(screen.getByRole('link', { name: 'Open payment page' }).getAttribute('href')).toBe(`#/pay/${'0'.repeat(32)}`);
    await fireEvent.click(screen.getByRole('button', { name: 'Close' }));
    expect(screen.queryByRole('complementary')).toBeNull();
  });
});
