import { describe, expect, it, vi } from 'vitest';
import { fireEvent, render, screen, waitFor, within } from '@testing-library/svelte';
import Invoices from './Invoices.svelte';
import type { PayApi } from '../lib/api';
import { PUBKEY, adminInvoice, info, invoice, stats } from '../test-helpers';

const second = adminInvoice({ id: 'f'.repeat(32), index: 1, status: 'confirmed', received_sat: 50_000, memo: 'order 9' });

function setup(api: Partial<Record<keyof PayApi, ReturnType<typeof vi.fn>>>, refreshMs = 60_000, copy = vi.fn().mockResolvedValue(undefined)) {
  const onCreated = vi.fn();
  const full = {
    getStats: vi.fn().mockResolvedValue(stats()),
    listInvoices: vi.fn().mockResolvedValue({ invoices: [second, adminInvoice()], next_before: null }),
    createInvoice: vi.fn().mockResolvedValue(invoice()),
    ...api,
  };
  const view = render(Invoices, { props: { api: full as unknown as PayApi, info: info({ is_admin: true }), onCreated, refreshMs, copy } });
  return { ...full, onCreated, copy, ...view };
}

describe('Invoices', () => {
  it('heads the page "Drops", counts each status and shows the first drop beside the list', async () => {
    const t = setup({});
    expect(screen.getByRole('heading', { level: 1 }).textContent).toBe('Drops');
    await waitFor(() => expect(screen.getAllByRole('row')).toHaveLength(3));
    expect(t.listInvoices).toHaveBeenCalledWith({});
    const counts = [...screen.getByRole('group', { name: 'Status' }).querySelectorAll('button')].map((b) => b.textContent);
    expect(counts).toEqual(['All10', 'Awaiting payment2', 'Confirming1', 'Paid3', 'Expired4']);
    expect(screen.getByRole('button', { name: 'Paid' })).toBeTruthy();

    const rows = screen.getAllByRole('row');
    const first = within(rows[1]);
    expect(rows[1].className).toBe('on');
    expect(first.getByRole('button').getAttribute('aria-pressed')).toBe('true');
    expect(first.getByText('Paid')).toBeTruthy();
    expect(first.getByText('order 9')).toBeTruthy();
    expect(first.getByText('11-14 22:13').title).toBe('2023-11-14 22:13 UTC');
    const panel = screen.getByRole('complementary', { name: 'Drop 0001' });
    expect(panel.textContent).toContain('Paid and confirmed.');
    expect(within(panel).getByTitle(PUBKEY).textContent).toBe('79be667e…f81798');
    expect(screen.getByRole('link', { name: 'Open payment page' }).getAttribute('href')).toBe(`#/pay/${'f'.repeat(32)}`);
    // A paid drop needs no QR code.
    expect(panel.querySelector('svg.qr')).toBeNull();
    expect(screen.queryByRole('button', { name: 'Load more' })).toBeNull();

    await fireEvent.click(within(rows[2]).getByRole('button'));
    expect(screen.getAllByRole('row')[2].className).toBe('on');
    expect(screen.getByRole('complementary', { name: 'Drop 0000' }).querySelector('svg.qr')).not.toBeNull();
  });

  it('says when there are no invoices, or none match the search', async () => {
    setup({ listInvoices: vi.fn().mockResolvedValue({ invoices: [], next_before: null }) });
    expect(await screen.findByText('No invoices yet.')).toBeTruthy();
    expect(screen.queryByRole('complementary')).toBeNull();

    document.body.innerHTML = '';
    setup({});
    await waitFor(() => expect(screen.getAllByRole('row')).toHaveLength(3));
    await fireEvent.input(screen.getByLabelText('Find a drop'), { target: { value: 'ORDER' } });
    expect(screen.getAllByRole('row')).toHaveLength(2);
    await fireEvent.input(screen.getByLabelText('Find a drop'), { target: { value: '0000' } });
    expect(screen.getByRole('complementary', { name: 'Drop 0000' })).toBeTruthy();
    await fireEvent.input(screen.getByLabelText('Find a drop'), { target: { value: 'nothing like it' } });
    expect(screen.getByText('No drop matches the search.')).toBeTruthy();
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

  it('opens the new invoice form from the page head', async () => {
    const t = setup({});
    expect(screen.queryByLabelText('Amount (sats)')).toBeNull();
    const toggle = screen.getByRole('button', { name: 'New invoice' });
    await fireEvent.click(toggle);
    expect(toggle.getAttribute('aria-expanded')).toBe('true');
    await fireEvent.input(screen.getByLabelText('Amount (sats)'), { target: { value: '50000' } });
    await fireEvent.click(screen.getByRole('button', { name: 'Create invoice' }));
    await waitFor(() => expect(t.onCreated).toHaveBeenCalledWith(invoice()));
    await fireEvent.click(toggle);
    expect(screen.queryByLabelText('Amount (sats)')).toBeNull();
  });

  it('shows a drop with its progress and what it is waiting for', async () => {
    const statuses = ['pending', 'detected', 'confirmed', 'expired'] as const;
    const rows = statuses.map((status, i) =>
      adminInvoice({ id: `${i}`.repeat(32), index: 0x7f3a + i, status, memo: i === 0 ? 'notebook' : '', confirmations_required: i === 1 ? 3 : 1 }),
    );
    rows.push(adminInvoice({ id: '9'.repeat(32), index: 0x7f40, status: 'detected' }));
    setup({ listInvoices: vi.fn().mockResolvedValue({ invoices: rows, next_before: null }) });
    const expected: Record<string, [string[], string]> = {
      '7F3A': [['done', 'transit', 'todo'], 'Waiting for payment. Nothing seen on the chain yet.'],
      '7F3B': [['done', 'done', 'transit'], 'Payment seen. Waiting for 3 confirmations.'],
      '7F3C': [['done', 'done', 'done'], 'Paid and confirmed.'],
      '7F3D': [['done', 'todo', 'todo'], 'Expired unpaid. A late payment would still be credited.'],
      '7F40': [['done', 'done', 'transit'], 'Payment seen. Waiting for 1 confirmation.'],
    };
    for (const [id, [classes, line]] of Object.entries(expected)) {
      await fireEvent.click(await screen.findByRole('button', { name: id }));
      const panel = screen.getByRole('complementary', { name: `Drop ${id}` });
      expect([...panel.querySelectorAll('ol.progress li')].map((li) => li.className)).toEqual(classes);
      expect(panel.textContent).toContain(line);
    }
    await fireEvent.click(screen.getByRole('button', { name: '7F3A' }));
    expect(screen.getByText('notebook', { selector: '.memo-text' })).toBeTruthy();
    expect(screen.getByRole('link', { name: 'Open payment page' }).getAttribute('href')).toBe(`#/pay/${'0'.repeat(32)}`);
  });

  it('copies the address and the payment link', async () => {
    const t = setup({});
    await fireEvent.click(await screen.findByRole('button', { name: 'Copy address' }));
    expect(t.copy).toHaveBeenLastCalledWith(second.address);
    expect(screen.getByRole('button', { name: 'Copied' })).toBeTruthy();
    await fireEvent.click(screen.getByRole('button', { name: 'Copy payment link' }));
    expect(t.copy).toHaveBeenLastCalledWith(`${location.origin}${location.pathname}#/pay/${'f'.repeat(32)}`);
    expect(screen.getByRole('button', { name: 'Copy address' })).toBeTruthy();

    document.body.innerHTML = '';
    setup({}, 60_000, vi.fn().mockRejectedValue(new Error('denied')));
    await fireEvent.click(await screen.findByRole('button', { name: 'Copy address' }));
    await waitFor(() => expect(screen.getByRole('alert').textContent).toContain('copy it by hand'));
  });

  it('uses the browser clipboard by default', async () => {
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.defineProperty(navigator, 'clipboard', { value: { writeText }, configurable: true });
    const api = { getStats: vi.fn().mockResolvedValue(stats()), listInvoices: vi.fn().mockResolvedValue({ invoices: [second], next_before: null }) };
    render(Invoices, { props: { api: api as unknown as PayApi, info: info({ is_admin: true }), onCreated: vi.fn() } });
    await fireEvent.click(await screen.findByRole('button', { name: 'Copy address' }));
    expect(writeText).toHaveBeenCalledWith(second.address);
  });
});
