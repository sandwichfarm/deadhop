// Types for the Svelte 4 components (class components, as Svelte 4 expects).
import { SvelteComponent } from 'svelte';
import type { CheckoutEvents, CheckoutState, Deadhop, Labels } from '@deadhop/client';

export { createDeadhop, createCheckout } from '@deadhop/client';

interface Common extends CheckoutEvents {
  client: Pick<Deadhop, 'createInvoice' | 'getInvoice'>;
  pollMs?: number;
  labels?: Partial<Labels>;
  class?: string;
  copy?: (text: string) => Promise<void>;
}

export interface InvoiceProps extends Common {
  id: string;
}
export interface PayButtonProps extends Common {
  amount: number;
  memo?: string;
  expirySecs?: number;
  label?: string;
}
export interface InvoiceFormProps extends Common {
  defaultAmount?: number;
  defaultMemo?: string;
  showMemo?: boolean;
  expirySecs?: number;
}
export interface InvoiceCardProps {
  state: CheckoutState;
  labels: Labels;
  copy?: (text: string) => Promise<void>;
}

export class Invoice extends SvelteComponent<InvoiceProps> {}
export class InvoiceForm extends SvelteComponent<InvoiceFormProps> {}
export class PayButton extends SvelteComponent<PayButtonProps> {}
export class InvoiceCard extends SvelteComponent<InvoiceCardProps> {}
