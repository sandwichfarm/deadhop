// Server rendering: importing and rendering must not touch browser APIs.
import { describe, expect, it } from 'vitest';
import { render } from 'svelte/server';
import * as svelte5 from '../src/svelte5/index.ts';
import * as svelte4 from '../src/svelte4/index.js';
import { fakeClient, ID } from '../../shared/test/fixtures.ts';

describe('Svelte server rendering', () => {
  for (const [name, kit] of [['Svelte 5', svelte5], ['Svelte 4 syntax', svelte4]] as const) {
    it(`renders the initial markup without a browser (${name})`, () => {
      const client = fakeClient();
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      const html = (c: any, props: Record<string, unknown>) => render(c, { props }).body;
      expect(html(kit.PayButton, { client, amount: 5000 })).toContain('Pay 5,000 sats');
      expect(html(kit.InvoiceForm, { client })).toContain('Amount (sats)');
      expect(html(kit.Invoice, { client, id: ID })).toContain('deadhop');
    });
  }
});
