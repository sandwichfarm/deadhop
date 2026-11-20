// Renders one checkout state. Shared by Invoice, InvoiceForm and PayButton.
import { Show, createMemo, createSignal, type JSX } from 'solid-js';
import { invoiceView, progressMessage, type CheckoutState, type Labels } from '@deadhop/client';

export interface InvoiceCardProps {
  state: CheckoutState;
  labels: Labels;
  copy?: (text: string) => Promise<void>;
}

function clipboard(text: string): Promise<void> {
  return navigator.clipboard.writeText(text);
}

export function InvoiceCard(props: InvoiceCardProps): JSX.Element {
  const [copied, setCopied] = createSignal(false);
  const view = createMemo(() => (props.state.invoice ? invoiceView(props.state.invoice, props.state.now) : null));
  const copy = (address: string) => void (props.copy ?? clipboard)(address).then(() => setCopied(true));
  return (
    <Show
      when={view()}
      fallback={
        <Show when={progressMessage(props.state)} fallback={<Show when={props.state.error}>{(e) => <p class="deadhop-error" role="alert">{e().message}</p>}</Show>}>
          {(message) => (
            <div class="deadhop-invoice">
              <p class="deadhop-status" role="status">{message()}</p>
            </div>
          )}
        </Show>
      }
    >
      {(v) => (
        <div class="deadhop-invoice" data-status={v().status}>
          <p class="deadhop-status" role="status">{v().statusLabel}</p>
          <p class="deadhop-amount">{v().amountBtc}</p>
          <p class="deadhop-sats">{v().amountSats}</p>
          <Show when={v().memo}>
            {(memo) => (
              <p class="deadhop-memo">
                <span class="deadhop-memo-label">{props.labels.memoNote}</span> <span>{memo()}</span>
              </p>
            )}
          </Show>
          <Show when={v().qr}>
            {(qr) => (
              <>
                <a class="deadhop-qr" href={v().bip21} aria-label={props.labels.openInWallet}>
                  <svg class="deadhop-qr-code" viewBox={`0 0 ${qr().size} ${qr().size}`} role="img" aria-label={props.labels.qr} shape-rendering="crispEdges">
                    <rect width={qr().size} height={qr().size} fill="#ffffff" />
                    <path d={qr().path} fill="#000000" />
                  </svg>
                </a>
                <p class="deadhop-address">{v().address}</p>
                <button type="button" class="deadhop-copy" onClick={() => copy(v().address)}>
                  {copied() ? props.labels.copied : props.labels.copy}
                </button>
                <Show when={v().expiry}>
                  <p class="deadhop-expiry">{v().expiry}</p>
                </Show>
              </>
            )}
          </Show>
          <Show when={v().received}>{(r) => <p class="deadhop-received">{r()}</p>}</Show>
          <Show when={props.state.error}>{(e) => <p class="deadhop-error" role="alert">{e().message}</p>}</Show>
        </div>
      )}
    </Show>
  );
}
