'use client';
import { useEffect, type ReactElement } from 'react';
import { LABELS } from '@cvmpay/client';
import { InvoiceCard } from './InvoiceCard.tsx';
import type { CommonProps } from './props.ts';
import { useCheckout } from './useCheckout.ts';

export interface InvoiceProps extends CommonProps {
  /** Id of an invoice created elsewhere (for example by your backend). */
  id: string;
}

/** Shows an existing invoice and follows it until it is paid. */
export function Invoice({ id, client, pollMs, labels, className, copy, ...events }: InvoiceProps): ReactElement {
  const [state, checkout] = useCheckout(client, { pollMs, ...events });
  useEffect(() => {
    checkout.reset();
    void checkout.watch(id);
  }, [checkout, id]);
  return (
    <div className={className ? `cvmpay ${className}` : 'cvmpay'}>
      <InvoiceCard state={state} labels={{ ...LABELS, ...labels }} copy={copy} />
    </div>
  );
}
