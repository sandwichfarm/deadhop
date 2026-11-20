'use client';
// Renders one checkout state. Shared by Invoice, InvoiceForm and PayButton.
import { useState, type ReactElement } from 'react';
import { invoiceView, progressMessage, type CheckoutState, type Labels } from '@deadhop/client';

export interface InvoiceCardProps {
  state: CheckoutState;
  labels: Labels;
  copy?: (text: string) => Promise<void>;
}

function clipboard(text: string): Promise<void> {
  return navigator.clipboard.writeText(text);
}

export function InvoiceCard({ state, labels, copy = clipboard }: InvoiceCardProps): ReactElement | null {
  const [copied, setCopied] = useState(false);
  const progress = progressMessage(state);
  if (!state.invoice) {
    if (progress) {
      return (
        <div className="deadhop-invoice">
          <p className="deadhop-status" role="status">{progress}</p>
        </div>
      );
    }
    return state.error ? <p className="deadhop-error" role="alert">{state.error.message}</p> : null;
  }
  const v = invoiceView(state.invoice, state.now);
  return (
    <div className="deadhop-invoice" data-status={v.status}>
      <p className="deadhop-status" role="status">{v.statusLabel}</p>
      <p className="deadhop-amount">{v.amountBtc}</p>
      <p className="deadhop-sats">{v.amountSats}</p>
      {v.memo !== null && (
        <p className="deadhop-memo">
          <span className="deadhop-memo-label">{labels.memoNote}</span> <span>{v.memo}</span>
        </p>
      )}
      {v.payable && v.qr && (
        <>
          <a className="deadhop-qr" href={v.bip21} aria-label={labels.openInWallet}>
            <svg className="deadhop-qr-code" viewBox={`0 0 ${v.qr.size} ${v.qr.size}`} role="img" aria-label={labels.qr} shapeRendering="crispEdges">
              <rect width={v.qr.size} height={v.qr.size} fill="#ffffff" />
              <path d={v.qr.path} fill="#000000" />
            </svg>
          </a>
          <p className="deadhop-address">{v.address}</p>
          <button type="button" className="deadhop-copy" onClick={() => void copy(v.address).then(() => setCopied(true))}>
            {copied ? labels.copied : labels.copy}
          </button>
          {v.expiry && <p className="deadhop-expiry">{v.expiry}</p>}
        </>
      )}
      {v.received && <p className="deadhop-received">{v.received}</p>}
      {state.error && <p className="deadhop-error" role="alert">{state.error.message}</p>}
    </div>
  );
}
