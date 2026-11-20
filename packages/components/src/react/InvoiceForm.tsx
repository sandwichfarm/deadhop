'use client';
import { useId, useState, type FormEvent, type ReactElement } from 'react';
import { LABELS, parseSats } from '@deadhop/client';
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
export function InvoiceForm({
  defaultAmount,
  defaultMemo = '',
  showMemo = true,
  expirySecs,
  client,
  pollMs,
  labels,
  className,
  copy,
  ...events
}: InvoiceFormProps): ReactElement {
  const [state, checkout] = useCheckout(client, { pollMs, ...events });
  const all = { ...LABELS, ...labels };
  const [amount, setAmount] = useState(defaultAmount ? String(defaultAmount) : '');
  const [memo, setMemo] = useState(defaultMemo);
  const [invalid, setInvalid] = useState(false);
  const ids = useId();
  const editing = state.phase === 'idle' || state.phase === 'error';

  function submit(e: FormEvent) {
    e.preventDefault();
    const sats = parseSats(amount);
    setInvalid(sats === null);
    if (sats === null) return;
    checkout.reset();
    void checkout.create({ amount_sat: sats, ...(memo.trim() ? { memo: memo.trim() } : {}), ...(expirySecs ? { expiry_secs: expirySecs } : {}) });
  }

  return (
    <div className={className ? `deadhop ${className}` : 'deadhop'}>
      {editing ? (
        <form className="deadhop-form" onSubmit={submit} noValidate>
          <label htmlFor={`${ids}-amount`}>{all.amount}</label>
          <input id={`${ids}-amount`} name="amount" inputMode="numeric" autoComplete="off" value={amount} onChange={(e) => setAmount(e.target.value)} />
          {showMemo && (
            <>
              <label htmlFor={`${ids}-memo`}>{all.memo}</label>
              <input id={`${ids}-memo`} name="memo" maxLength={200} value={memo} onChange={(e) => setMemo(e.target.value)} />
            </>
          )}
          {invalid && <p className="deadhop-error" role="alert">{all.invalidAmount}</p>}
          <button type="submit" className="deadhop-create">{all.create}</button>
        </form>
      ) : (
        <button type="button" className="deadhop-again" onClick={() => checkout.reset()}>
          {all.again}
        </button>
      )}
      <InvoiceCard state={state} labels={all} copy={copy} />
    </div>
  );
}
