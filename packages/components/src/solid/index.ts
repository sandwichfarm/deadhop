/**
 * Solid components for cvmpay: `PayButton`, `InvoiceForm`, `Invoice`, and the
 * headless `useCheckout` primitive. Pair with `createCvmpay` from `@cvmpay/client`.
 *
 * @module
 */
export { Invoice, type InvoiceProps } from './Invoice.tsx';
export { InvoiceForm, type InvoiceFormProps } from './InvoiceForm.tsx';
export { PayButton, type PayButtonProps } from './PayButton.tsx';
export { InvoiceCard, type InvoiceCardProps } from './InvoiceCard.tsx';
export { useCheckout, type UseCheckoutOptions } from './useCheckout.ts';
export type { CommonProps } from './props.ts';
export { createCvmpay, type Cvmpay, type CvmpayOptions, type Invoice as InvoiceData } from '@cvmpay/client';
