// Server rendering: importing and rendering must not touch browser APIs.
import { describe, expect, it } from 'vitest';
import { renderToString } from 'react-dom/server';
import { Invoice, InvoiceForm, PayButton } from '../../src/react/index.ts';
import { fakeClient, ID } from '../fixtures.ts';

describe('React server rendering', () => {
  it('renders the initial markup without a browser', () => {
    const client = fakeClient();
    expect(renderToString(<PayButton client={client} amount={5000} />)).toContain('Pay 5,000 sats');
    expect(renderToString(<InvoiceForm client={client} />)).toContain('Amount (sats)');
    expect(renderToString(<Invoice client={client} id={ID} />)).toContain('class="cvmpay"');
    expect(client.getInvoice).not.toHaveBeenCalled();
  });
});
