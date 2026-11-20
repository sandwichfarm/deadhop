// @vitest-environment node
// Server rendering with the Svelte 4 compiler (components compiled for SSR).
import { describe, expect, it } from 'vitest';
import * as kit from '../src/svelte4/index.js';
import { ID, fakeClient } from '../../shared/test/fixtures.ts';

describe('Svelte 4 server rendering', () => {
  it('renders the initial markup without a browser', () => {
    const client = fakeClient();
    expect(kit.PayButton.render({ client, amount: 5000 }).html).toContain('Pay 5,000 sats');
    expect(kit.InvoiceForm.render({ client }).html).toContain('Amount (sats)');
    expect(kit.Invoice.render({ client, id: ID }).html).toContain('deadhop');
  });
});
