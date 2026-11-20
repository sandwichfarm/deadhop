// Server rendering: importing and rendering must not touch browser APIs.
import { describe, expect, it } from 'vitest';
import { renderToString } from 'solid-js/web';
import { Invoice, InvoiceForm, PayButton } from '../src/index.ts';
import { fakeClient, ID } from '../../shared/test/fixtures.ts';

describe('Solid server rendering', () => {
  it('renders the initial markup without a browser', () => {
    const client = fakeClient();
    expect(renderToString(() => <PayButton client={client} amount={5000} />)).toContain('Pay 5,000 sats');
    expect(renderToString(() => <InvoiceForm client={client} />)).toContain('Amount (sats)');
    expect(renderToString(() => <Invoice client={client} id={ID} />)).toContain('deadhop');
  });
});
