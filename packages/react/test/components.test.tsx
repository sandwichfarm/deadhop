import { StrictMode } from 'react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { Invoice, InvoiceCard, InvoiceForm, PayButton } from '../src/index.ts';
import { LABELS } from '@deadhop/client';
import { ADDRESS, ID, confirmed, detected, fakeClient, invoice, notFound, unauthorized } from '../../shared/test/fixtures.ts';

afterEach(cleanup);

describe('PayButton (React)', () => {
  it('creates an invoice on click and follows it until paid', async () => {
    const client = fakeClient([invoice(), detected(), confirmed()]);
    const release = client.holdCreate();
    const events = { onCreated: vi.fn(), onDetected: vi.fn(), onPaid: vi.fn(), onError: vi.fn() };
    const copy = vi.fn(async () => undefined);
    const { container } = render(<PayButton client={client} amount={5000} memo="Order 42" expirySecs={600} pollMs={10} copy={copy} className="shop" {...events} />);
    expect(container.firstElementChild?.className).toBe('deadhop shop');
    fireEvent.click(screen.getByRole('button', { name: 'Pay 5,000 sats' }));
    expect(await screen.findByText('Preparing invoice…')).toBeTruthy();
    release();
    expect(await screen.findByText('Awaiting payment')).toBeTruthy();
    expect(client.createInvoice).toHaveBeenCalledWith({ amount_sat: 5000, memo: 'Order 42', expiry_secs: 600 });
    expect(screen.getByText('0.00005000 BTC')).toBeTruthy();
    expect(screen.getByText('5,000 sats')).toBeTruthy();
    expect(screen.getByText('Note from the invoice creator:')).toBeTruthy();
    expect(screen.getByText('Order 42')).toBeTruthy();
    expect(screen.getByRole('img', { name: 'Payment QR code' })).toBeTruthy();
    expect(screen.getByRole('link', { name: 'Open in wallet' }).getAttribute('href')).toBe(`bitcoin:${ADDRESS}?amount=0.00005000`);
    expect(screen.getByText(ADDRESS)).toBeTruthy();
    expect(screen.getByText(/^Expires in/)).toBeTruthy();
    expect(screen.queryByRole('button', { name: /^Pay/ })).toBeNull();

    fireEvent.click(screen.getByRole('button', { name: 'Copy address' }));
    expect(await screen.findByRole('button', { name: 'Copied' })).toBeTruthy();
    expect(copy).toHaveBeenCalledWith(ADDRESS);

    expect(await screen.findByText('Payment seen, confirming')).toBeTruthy();
    expect(await screen.findByText('Paid')).toBeTruthy();
    expect(screen.queryByRole('img')).toBeNull();
    expect(screen.getByText(/^Received 5,000 sats/)).toBeTruthy();
    expect(events.onCreated).toHaveBeenCalledTimes(1);
    expect(events.onDetected).toHaveBeenCalledTimes(1);
    expect(events.onPaid).toHaveBeenCalledWith(expect.objectContaining({ id: ID, status: 'confirmed' }));
    expect(events.onError).not.toHaveBeenCalled();
  });

  it('shows a refusal and lets the payer try again', async () => {
    const client = fakeClient();
    client.createInvoice.mockImplementationOnce(async () => {
      throw unauthorized();
    });
    const onError = vi.fn();
    render(
      <PayButton client={client} amount={21} onError={onError} labels={{ copy: 'Copy' }}>
        Buy
      </PayButton>,
    );
    fireEvent.click(screen.getByRole('button', { name: 'Buy' }));
    expect((await screen.findByRole('alert')).textContent).toBe('This key is not allowed to do that.');
    expect(onError).toHaveBeenCalledTimes(1);
    fireEvent.click(screen.getByRole('button', { name: 'Buy' }));
    expect(await screen.findByRole('button', { name: 'Copy' })).toBeTruthy();
    expect(client.createInvoice).toHaveBeenLastCalledWith({ amount_sat: 21 });
  });

  it('survives StrictMode double mounting and uses the clipboard by default', async () => {
    const writeText = vi.fn(async () => undefined);
    Object.defineProperty(navigator, 'clipboard', { value: { writeText }, configurable: true });
    const client = fakeClient();
    render(
      <StrictMode>
        <PayButton client={client} amount={5000} />
      </StrictMode>,
    );
    fireEvent.click(screen.getByRole('button', { name: 'Pay 5,000 sats' }));
    fireEvent.click(await screen.findByRole('button', { name: 'Copy address' }));
    await waitFor(() => expect(writeText).toHaveBeenCalledWith(ADDRESS));
  });
});

describe('InvoiceForm (React)', () => {
  it('checks the amount, creates the invoice and starts over', async () => {
    const client = fakeClient();
    const onCreated = vi.fn();
    render(<InvoiceForm client={client} onCreated={onCreated} expirySecs={900} className="x" />);
    fireEvent.change(screen.getByLabelText('Amount (sats)'), { target: { value: 'lots' } });
    fireEvent.click(screen.getByRole('button', { name: 'Create invoice' }));
    expect((await screen.findByRole('alert')).textContent).toBe('Enter a whole number of sats.');
    expect(client.createInvoice).not.toHaveBeenCalled();

    fireEvent.change(screen.getByLabelText('Amount (sats)'), { target: { value: '5,000' } });
    fireEvent.change(screen.getByLabelText('Note (optional)'), { target: { value: '  Order 42 ' } });
    fireEvent.click(screen.getByRole('button', { name: 'Create invoice' }));
    expect(await screen.findByText('Awaiting payment')).toBeTruthy();
    expect(client.createInvoice).toHaveBeenCalledWith({ amount_sat: 5000, memo: 'Order 42', expiry_secs: 900 });
    expect(onCreated).toHaveBeenCalledTimes(1);
    expect(screen.queryByRole('alert')).toBeNull();

    fireEvent.click(screen.getByRole('button', { name: 'New invoice' }));
    expect(screen.getByLabelText('Amount (sats)')).toBeTruthy();
    expect(screen.queryByText('Awaiting payment')).toBeNull();
  });

  it('can start filled in, without a note field, and reports failures', async () => {
    const client = fakeClient([invoice()], unauthorized());
    render(<InvoiceForm client={client} defaultAmount={2100} defaultMemo="tip" showMemo={false} labels={{ create: 'Go' }} />);
    expect((screen.getByLabelText('Amount (sats)') as HTMLInputElement).value).toBe('2100');
    expect(screen.queryByLabelText('Note (optional)')).toBeNull();
    fireEvent.click(screen.getByRole('button', { name: 'Go' }));
    expect((await screen.findByRole('alert')).textContent).toBe('This key is not allowed to do that.');
    expect(client.createInvoice).toHaveBeenCalledWith({ amount_sat: 2100, memo: 'tip' });
    expect(screen.getByRole('button', { name: 'Go' })).toBeTruthy();
  });

  it('reports each step until paid', async () => {
    const events = { onDetected: vi.fn(), onPaid: vi.fn(), onError: vi.fn() };
    render(<InvoiceForm client={fakeClient([detected(), confirmed()])} defaultAmount={5000} pollMs={10} {...events} />);
    fireEvent.click(screen.getByRole('button', { name: 'Create invoice' }));
    expect(await screen.findByText('Paid')).toBeTruthy();
    expect(events.onDetected).toHaveBeenCalledTimes(1);
    expect(events.onPaid).toHaveBeenCalledTimes(1);
    cleanup();
    render(<InvoiceForm client={fakeClient([invoice()], unauthorized())} defaultAmount={5000} onError={events.onError} />);
    fireEvent.click(screen.getByRole('button', { name: 'Create invoice' }));
    await waitFor(() => expect(events.onError).toHaveBeenCalledTimes(1));
  });

  it('leaves out an empty note', async () => {
    const client = fakeClient();
    render(<InvoiceForm client={client} defaultAmount={1000} />);
    fireEvent.click(screen.getByRole('button', { name: 'Create invoice' }));
    await screen.findByText('Awaiting payment');
    expect(client.createInvoice).toHaveBeenCalledWith({ amount_sat: 1000 });
  });
});

describe('Invoice (React)', () => {
  it('follows an invoice by id, switches ids and stops when unmounted', async () => {
    const client = fakeClient([invoice({ memo: '' })]);
    const { rerender, unmount, container } = render(<Invoice client={client} id={ID} pollMs={10} />);
    expect(screen.getByText('Loading invoice…')).toBeTruthy();
    expect(await screen.findByText('Awaiting payment')).toBeTruthy();
    expect(screen.queryByText('Note from the invoice creator:')).toBeNull();
    expect(container.firstElementChild?.className).toBe('deadhop');
    const other = 'f'.repeat(32);
    rerender(<Invoice client={client} id={other} pollMs={10} />);
    await waitFor(() => expect(client.getInvoice).toHaveBeenCalledWith(other));
    unmount();
    const calls = client.getInvoice.mock.calls.length;
    await new Promise((r) => setTimeout(r, 40));
    expect(client.getInvoice.mock.calls.length).toBe(calls);
  });

  it('shows why an invoice cannot be shown, and expired invoices without payment details', async () => {
    render(<Invoice client={fakeClient([notFound()])} id={ID} className="y" />);
    expect((await screen.findByRole('alert')).textContent).toBe('Invoice not found.');
    cleanup();
    render(<Invoice client={fakeClient([invoice({ status: 'expired' })])} id={ID} />);
    expect(await screen.findByText('Expired')).toBeTruthy();
    expect(screen.queryByRole('img')).toBeNull();
  });

  it('reports each step of an invoice created elsewhere', async () => {
    const events = { onCreated: vi.fn(), onDetected: vi.fn(), onPaid: vi.fn(), onError: vi.fn() };
    render(<Invoice client={fakeClient([detected(), confirmed()])} id={ID} pollMs={10} {...events} />);
    expect(await screen.findByText('Payment seen, confirming')).toBeTruthy();
    expect(screen.getByRole('img')).toBeTruthy();
    expect(screen.queryByText(/^Expires in/)).toBeNull();
    expect(await screen.findByText('Paid')).toBeTruthy();
    expect(events.onCreated).toHaveBeenCalledTimes(1);
    expect(events.onDetected).toHaveBeenCalledTimes(1);
    expect(events.onPaid).toHaveBeenCalledTimes(1);
    cleanup();
    const onError = vi.fn();
    render(<Invoice client={fakeClient([notFound()])} id={ID} onError={onError} />);
    await waitFor(() => expect(onError).toHaveBeenCalledTimes(1));
  });

  it('keeps showing the invoice when a poll fails', async () => {
    render(<Invoice client={fakeClient([invoice(), new Error('socket'), invoice()])} id={ID} pollMs={10} />);
    expect((await screen.findByRole('alert')).textContent).toBe('Could not reach the service.');
    expect(screen.getByText('Awaiting payment')).toBeTruthy();
  });
});

describe('InvoiceCard (React)', () => {
  it('renders nothing before anything happens', () => {
    const { container } = render(<InvoiceCard state={{ phase: 'idle', invoice: null, error: null, now: 0 }} labels={LABELS} />);
    expect(container.innerHTML).toBe('');
  });
});
