// What a checkout looks like, as plain data. Every framework's components
// render exactly this, so they all show the same thing.
import type { CheckoutState } from './checkout.ts';
import { formatBtc, formatSats, receivedSummary, statusLabel, timeLeft } from './format.ts';
import { qrShape, type QrShape } from './qr.ts';
import type { Invoice, InvoiceStatus } from './schemas.ts';

/** Rendering of one invoice. */
export interface InvoiceView {
  status: InvoiceStatus;
  statusLabel: string;
  amountBtc: string;
  amountSats: string;
  /** The creator's note, or null. Plain text from a possibly untrusted creator. */
  memo: string | null;
  /** Whether to show the QR code, address and copy button. */
  payable: boolean;
  bip21: string;
  address: string;
  /** QR code of `bip21` (null when not payable). */
  qr: QrShape | null;
  /** `Expires in 59m 10s`, or empty. */
  expiry: string;
  /** What has arrived so far, or null before anything has. */
  received: string | null;
}

/** Build the view of `invoice` at Unix time `now`. */
export function invoiceView(invoice: Invoice, now: number): InvoiceView {
  const payable = invoice.status === 'pending' || invoice.status === 'detected';
  const left = invoice.status === 'pending' ? timeLeft(invoice.expires_at, now) : '';
  return {
    status: invoice.status,
    statusLabel: statusLabel(invoice.status),
    amountBtc: formatBtc(invoice.amount_sat),
    amountSats: formatSats(invoice.amount_sat),
    memo: invoice.memo === '' ? null : invoice.memo,
    payable,
    bip21: invoice.bip21,
    address: invoice.address,
    qr: payable ? qrShape(invoice.bip21) : null,
    expiry: left ? `Expires in ${left}` : '',
    received: invoice.received_sat > 0 ? receivedSummary(invoice) : null,
  };
}

/** Status line while there is no invoice to show yet, or null. */
export function progressMessage(state: CheckoutState): string | null {
  if (state.phase === 'creating') return 'Preparing invoice…';
  if (state.phase === 'loading') return 'Loading invoice…';
  return null;
}

/** `Pay 5,000 sats`. */
export function payLabel(amountSat: number): string {
  return `Pay ${formatSats(amountSat)}`;
}

/** Parse an amount typed in sats (`5000`, `5,000`, `5 000`); null if it is not a positive whole number. */
export function parseSats(input: string): number | null {
  const digits = input.replace(/[\s,_]/g, '');
  if (!/^[0-9]{1,16}$/.test(digits)) return null;
  const n = Number(digits);
  return n > 0 && Number.isSafeInteger(n) ? n : null;
}

/** Labels used by the components; override any of them for another language. */
export interface Labels {
  pay: (amountSat: number) => string;
  create: string;
  amount: string;
  memo: string;
  memoNote: string;
  copy: string;
  copied: string;
  openInWallet: string;
  qr: string;
  invalidAmount: string;
  again: string;
}

/** English defaults. */
export const LABELS: Labels = {
  pay: payLabel,
  create: 'Create invoice',
  amount: 'Amount (sats)',
  memo: 'Note (optional)',
  memoNote: 'Note from the invoice creator:',
  copy: 'Copy address',
  copied: 'Copied',
  openInWallet: 'Open in wallet',
  qr: 'Payment QR code',
  invalidAmount: 'Enter a whole number of sats.',
  again: 'New invoice',
};
