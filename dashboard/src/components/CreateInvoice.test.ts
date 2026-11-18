import { describe, expect, it, vi } from 'vitest';
import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import CreateInvoice from './CreateInvoice.svelte';
import type { PayApi } from '../lib/api';
import { info, invoice } from '../test-helpers';

function setup(over: Parameters<typeof info>[0] = {}, createInvoice = vi.fn().mockResolvedValue(invoice())) {
  const onCreated = vi.fn();
  render(CreateInvoice, { props: { api: { createInvoice } as unknown as PayApi, info: info(over), onCreated } });
  const amount = screen.getByLabelText('Amount (sats)') as HTMLInputElement;
  const memo = screen.getByLabelText('Note (optional)') as HTMLInputElement;
  const submit = () => fireEvent.submit(amount.closest('form')!);
  return { createInvoice, onCreated, amount, memo, submit };
}

describe('CreateInvoice', () => {
  it('validates the amount before calling the service', async () => {
    const t = setup();
    for (const bad of ['', 'abc', '12.5', '999', '100000001']) {
      await fireEvent.input(t.amount, { target: { value: bad } });
      await t.submit();
      expect(screen.getByRole('alert').textContent, bad).toContain('between 1,000 sats and 100,000,000 sats');
    }
    expect(t.createInvoice).not.toHaveBeenCalled();
  });

  it('creates an invoice, reports it and resets the form', async () => {
    const t = setup();
    await fireEvent.input(t.amount, { target: { value: '50000' } });
    await fireEvent.input(t.memo, { target: { value: '  table 4 ' } });
    await t.submit();
    await waitFor(() => expect(t.onCreated).toHaveBeenCalledWith(invoice()));
    expect(t.createInvoice).toHaveBeenCalledWith({ amount_sat: 50000, memo: 'table 4' });
    expect(t.amount.value).toBe('');
    expect(t.memo.value).toBe('');
    expect(screen.queryByRole('alert')).toBeNull();
  });

  it('omits an empty note', async () => {
    const t = setup();
    await fireEvent.input(t.amount, { target: { value: '1000' } });
    await fireEvent.input(t.memo, { target: { value: '   ' } });
    await t.submit();
    await waitFor(() => expect(t.createInvoice).toHaveBeenCalledWith({ amount_sat: 1000 }));
  });

  it('tells the public that proof of work is being computed', async () => {
    let finish!: (value: unknown) => void;
    const pending = vi.fn(() => new Promise((resolve) => (finish = resolve)));
    const t = setup({ pow_bits: 20 }, pending);
    await fireEvent.input(t.amount, { target: { value: '5000' } });
    await t.submit();
    expect(screen.getByRole('status').textContent).toBe('Computing proof of work (20 bits)…');
    expect((screen.getByRole('button', { name: 'Create invoice' }) as HTMLButtonElement).disabled).toBe(true);
    finish(invoice());
    await waitFor(() => expect(screen.queryByRole('status')).toBeNull());
  });

  it('skips the proof-of-work notice for operators and when none is required', async () => {
    for (const over of [{ is_admin: true, pow_bits: 20 }, { pow_bits: 0 }]) {
      const pending = vi.fn(() => new Promise(() => undefined));
      const t = setup(over, pending);
      await fireEvent.input(t.amount, { target: { value: '5000' } });
      await t.submit();
      expect(screen.getByRole('status').textContent).toBe('Creating…');
      document.body.innerHTML = '';
    }
  });

  it('shows service errors', async () => {
    const t = setup({}, vi.fn().mockRejectedValue(new Error('Too many requests. Wait a minute and try again.')));
    await fireEvent.input(t.amount, { target: { value: '5000' } });
    await t.submit();
    await waitFor(() => expect(screen.getByRole('alert').textContent).toContain('Too many requests'));
    expect(t.onCreated).not.toHaveBeenCalled();
    expect(t.amount.value).toBe('5000');
  });
});
