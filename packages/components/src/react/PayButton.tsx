'use client';
import type { ReactElement } from 'react';
import { LABELS } from '@cvmpay/client';
import { InvoiceCard } from './InvoiceCard.tsx';
import type { CommonProps } from './props.ts';
import { useCheckout } from './useCheckout.ts';

export interface PayButtonProps extends CommonProps {
  /** Amount in sats. */
  amount: number;
  /** Shown to the payer. */
  memo?: string;
  /** Invoice lifetime in seconds (default set by the service). */
  expirySecs?: number;
  /** Button text (default `Pay 5,000 sats`). */
  children?: string;
}

/** A button that creates an invoice for `amount` when pressed, then shows it. */
export function PayButton({ amount, memo, expirySecs, children, client, pollMs, labels, className, copy, ...events }: PayButtonProps): ReactElement {
  const [state, checkout] = useCheckout(client, { pollMs, ...events });
  const all = { ...LABELS, ...labels };
  const idle = state.phase === 'idle' || state.phase === 'error';
  function pay() {
    checkout.reset();
    void checkout.create({ amount_sat: amount, ...(memo ? { memo } : {}), ...(expirySecs ? { expiry_secs: expirySecs } : {}) });
  }
  return (
    <div className={className ? `cvmpay ${className}` : 'cvmpay'}>
      {idle && (
        <button type="button" className="cvmpay-pay" onClick={pay}>
          {children ?? all.pay(amount)}
        </button>
      )}
      <InvoiceCard state={state} labels={all} copy={copy} />
    </div>
  );
}
