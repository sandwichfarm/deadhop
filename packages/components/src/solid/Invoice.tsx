import { createEffect, on, type JSX } from 'solid-js';
import { LABELS } from '@deadhop/client';
import { InvoiceCard } from './InvoiceCard.tsx';
import type { CommonProps } from './props.ts';
import { useCheckout } from './useCheckout.ts';

export interface InvoiceProps extends CommonProps {
  /** Id of an invoice created elsewhere (for example by your backend). */
  id: string;
}

/** Shows an existing invoice and follows it until it is paid. */
export function Invoice(props: InvoiceProps): JSX.Element {
  const [state, checkout] = useCheckout(props.client, props);
  createEffect(
    on(
      () => props.id,
      (id) => {
        checkout.reset();
        void checkout.watch(id);
      },
    ),
  );
  return (
    <div class={props.class ? `deadhop ${props.class}` : 'deadhop'}>
      <InvoiceCard state={state()} labels={{ ...LABELS, ...props.labels }} copy={props.copy} />
    </div>
  );
}
