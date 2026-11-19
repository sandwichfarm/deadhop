import { Show, type JSX } from 'solid-js';
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
export function PayButton(props: PayButtonProps): JSX.Element {
  const [state, checkout] = useCheckout(props.client, props);
  const labels = () => ({ ...LABELS, ...props.labels });
  const idle = () => state().phase === 'idle' || state().phase === 'error';
  function pay() {
    checkout.reset();
    void checkout.create({
      amount_sat: props.amount,
      ...(props.memo ? { memo: props.memo } : {}),
      ...(props.expirySecs ? { expiry_secs: props.expirySecs } : {}),
    });
  }
  return (
    <div class={props.class ? `cvmpay ${props.class}` : 'cvmpay'}>
      <Show when={idle()}>
        <button type="button" class="cvmpay-pay" onClick={pay}>
          {props.children ?? labels().pay(props.amount)}
        </button>
      </Show>
      <InvoiceCard state={state()} labels={labels()} copy={props.copy} />
    </div>
  );
}
