'use client';
// Renders one checkout state. Shared by Invoice, InvoiceForm and PayButton.
import { useState, type ReactElement } from 'react';
import { invoiceView, progressMessage, type CheckoutState, type Labels } from '@cvmpay/client';

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
        <div className="cvmpay-invoice">
          <p className="cvmpay-status" role="status">{progress}</p>
        </div>
      );
    }
    return state.error ? <p className="cvmpay-error" role="alert">{state.error.message}</p> : null;
  }
  const v = invoiceView(state.invoice, state.now);
  return (
    <div className="cvmpay-invoice" data-status={v.status}>
      <p className="cvmpay-status" role="status">{v.statusLabel}</p>
      <p className="cvmpay-amount">{v.amountBtc}</p>
      <p className="cvmpay-sats">{v.amountSats}</p>
      {v.memo !== null && (
        <p className="cvmpay-memo">
          <span className="cvmpay-memo-label">{labels.memoNote}</span> <span>{v.memo}</span>
        </p>
      )}
      {v.payable && v.qr && (
        <>
          <a className="cvmpay-qr" href={v.bip21} aria-label={labels.openInWallet}>
            <svg className="cvmpay-qr-code" viewBox={`0 0 ${v.qr.size} ${v.qr.size}`} role="img" aria-label={labels.qr} shapeRendering="crispEdges">
              <rect width={v.qr.size} height={v.qr.size} fill="#ffffff" />
              <path d={v.qr.path} fill="#000000" />
            </svg>
          </a>
          <p className="cvmpay-address">{v.address}</p>
          <button type="button" className="cvmpay-copy" onClick={() => void copy(v.address).then(() => setCopied(true))}>
            {copied ? labels.copied : labels.copy}
          </button>
          {v.expiry && <p className="cvmpay-expiry">{v.expiry}</p>}
        </>
      )}
      {v.received && <p className="cvmpay-received">{v.received}</p>}
      {state.error && <p className="cvmpay-error" role="alert">{state.error.message}</p>}
    </div>
  );
}
