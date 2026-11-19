import { Show, createSignal, createUniqueId, type JSX } from 'solid-js';
import { LABELS, parseSats } from '@cvmpay/client';
import { InvoiceCard } from './InvoiceCard.tsx';
import type { CommonProps } from './props.ts';
import { useCheckout } from './useCheckout.ts';

export interface InvoiceFormProps extends CommonProps {
  /** Amount filled in at first, in sats. */
  defaultAmount?: number;
  /** Note filled in at first. */
  defaultMemo?: string;
  /** Show the note field (default true). */
  showMemo?: boolean;
  /** Invoice lifetime in seconds (default set by the service). */
  expirySecs?: number;
}

/** A form to create an invoice for any amount, which then shows the invoice. */
export function InvoiceForm(props: InvoiceFormProps): JSX.Element {
  const [state, checkout] = useCheckout(props.client, props);
  const labels = () => ({ ...LABELS, ...props.labels });
  const [amount, setAmount] = createSignal(props.defaultAmount ? String(props.defaultAmount) : '');
  const [memo, setMemo] = createSignal(props.defaultMemo ?? '');
  const [invalid, setInvalid] = createSignal(false);
  const ids = createUniqueId();
  const editing = () => state().phase === 'idle' || state().phase === 'error';

  function submit(e: SubmitEvent) {
    e.preventDefault();
    const sats = parseSats(amount());
    setInvalid(sats === null);
    if (sats === null) return;
    const note = memo().trim();
    checkout.reset();
    void checkout.create({ amount_sat: sats, ...(note ? { memo: note } : {}), ...(props.expirySecs ? { expiry_secs: props.expirySecs } : {}) });
  }

  return (
    <div class={props.class ? `cvmpay ${props.class}` : 'cvmpay'}>
      <Show
        when={editing()}
        fallback={
          <button type="button" class="cvmpay-again" onClick={() => checkout.reset()}>
            {labels().again}
          </button>
        }
      >
        <form class="cvmpay-form" onSubmit={submit} novalidate>
          <label for={`${ids}-amount`}>{labels().amount}</label>
          <input id={`${ids}-amount`} name="amount" inputmode="numeric" autocomplete="off" value={amount()} onInput={(e) => setAmount(e.currentTarget.value)} />
          <Show when={props.showMemo ?? true}>
            <label for={`${ids}-memo`}>{labels().memo}</label>
            <input id={`${ids}-memo`} name="memo" maxLength={200} value={memo()} onInput={(e) => setMemo(e.currentTarget.value)} />
          </Show>
          <Show when={invalid()}>
            <p class="cvmpay-error" role="alert">{labels().invalidAmount}</p>
          </Show>
          <button type="submit" class="cvmpay-create">{labels().create}</button>
        </form>
      </Show>
      <InvoiceCard state={state()} labels={labels()} copy={props.copy} />
    </div>
  );
}
