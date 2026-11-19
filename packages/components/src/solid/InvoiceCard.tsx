// Renders one checkout state. Shared by Invoice, InvoiceForm and PayButton.
import { Show, createMemo, createSignal, type JSX } from 'solid-js';
import { invoiceView, progressMessage, type CheckoutState, type Labels } from '@cvmpay/client';

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
        <Show when={progressMessage(props.state)} fallback={<Show when={props.state.error}>{(e) => <p class="cvmpay-error" role="alert">{e().message}</p>}</Show>}>
          {(message) => (
            <div class="cvmpay-invoice">
              <p class="cvmpay-status" role="status">{message()}</p>
            </div>
          )}
        </Show>
      }
    >
      {(v) => (
        <div class="cvmpay-invoice" data-status={v().status}>
          <p class="cvmpay-status" role="status">{v().statusLabel}</p>
          <p class="cvmpay-amount">{v().amountBtc}</p>
          <p class="cvmpay-sats">{v().amountSats}</p>
          <Show when={v().memo}>
            {(memo) => (
              <p class="cvmpay-memo">
                <span class="cvmpay-memo-label">{props.labels.memoNote}</span> <span>{memo()}</span>
              </p>
            )}
          </Show>
          <Show when={v().qr}>
            {(qr) => (
              <>
                <a class="cvmpay-qr" href={v().bip21} aria-label={props.labels.openInWallet}>
                  <svg class="cvmpay-qr-code" viewBox={`0 0 ${qr().size} ${qr().size}`} role="img" aria-label={props.labels.qr} shape-rendering="crispEdges">
                    <rect width={qr().size} height={qr().size} fill="#ffffff" />
                    <path d={qr().path} fill="#000000" />
                  </svg>
                </a>
                <p class="cvmpay-address">{v().address}</p>
                <button type="button" class="cvmpay-copy" onClick={() => copy(v().address)}>
                  {copied() ? props.labels.copied : props.labels.copy}
                </button>
                <Show when={v().expiry}>
                  <p class="cvmpay-expiry">{v().expiry}</p>
                </Show>
              </>
            )}
          </Show>
          <Show when={v().received}>{(r) => <p class="cvmpay-received">{r()}</p>}</Show>
          <Show when={props.state.error}>{(e) => <p class="cvmpay-error" role="alert">{e().message}</p>}</Show>
        </div>
      )}
    </Show>
  );
}
