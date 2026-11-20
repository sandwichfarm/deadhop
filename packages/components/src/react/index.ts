/**
 * React components for deadhop: `PayButton`, `InvoiceForm`, `Invoice`, and the
 * headless `useCheckout` hook. Pair with `createDeadhop` from `@deadhop/client`.
 *
 * @module
 */
export { Invoice, type InvoiceProps } from './Invoice.tsx';
export { InvoiceForm, type InvoiceFormProps } from './InvoiceForm.tsx';
export { PayButton, type PayButtonProps } from './PayButton.tsx';
export { InvoiceCard, type InvoiceCardProps } from './InvoiceCard.tsx';
export { useCheckout, type UseCheckoutOptions } from './useCheckout.ts';
export type { CommonProps } from './props.ts';
export { createDeadhop, type Deadhop, type DeadhopOptions, type Invoice as InvoiceData } from '@deadhop/client';
