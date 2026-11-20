// NIP-13 proof of work, mined into the request event before it is signed.
import { sha256 } from '@noble/hashes/sha2.js';
import { bytesToHex, utf8ToBytes } from '@noble/hashes/utils.js';
import type { NostrSigner } from '@contextvm/sdk';

/** ContextVM request/response event kind. */
export const CTXVM_KIND = 25910;

export interface UnsignedEvent {
  pubkey?: string;
  kind: number;
  tags: string[][];
  content: string;
  created_at: number;
}

/** Number of leading zero bits of a hex string. */
export function leadingZeroBits(hex: string): number {
  let bits = 0;
  for (const ch of hex) {
    const nibble = parseInt(ch, 16);
    if (nibble === 0) {
      bits += 4;
    } else {
      bits += Math.clz32(nibble) - 28;
      break;
    }
  }
  return bits;
}

/** NIP-01 event id. */
export function eventId(pubkey: string, ev: UnsignedEvent): string {
  const serialized = JSON.stringify([0, pubkey, ev.created_at, ev.kind, ev.tags, ev.content]);
  return bytesToHex(sha256(utf8ToBytes(serialized)));
}

/** Most work this page will attempt. Beyond this a browser would run for hours. */
export const MAX_POW_BITS = 28;

/** Raised when the required work cannot be produced here. */
export class PowError extends Error {
  constructor(message: string) {
    super(message);
    this.name = 'PowError';
  }
}

export interface MineOptions {
  /** Hashes between yields to the event loop, so the page stays responsive. */
  batch?: number;
  onProgress?: (hashes: number) => void;
  /** Give up after this long, so a tab never mines forever. */
  maxMs?: number;
  /** Clock, in milliseconds (injectable for tests). */
  now?: () => number;
}

/** Find a `nonce` tag that gives the event id `bits` leading zero bits. */
export async function mine(
  pubkey: string,
  ev: UnsignedEvent,
  bits: number,
  opts: MineOptions = {},
): Promise<UnsignedEvent> {
  if (bits > MAX_POW_BITS) {
    throw new PowError(`The service asks for ${bits} bits of proof of work; this app stops at ${MAX_POW_BITS}.`);
  }
  const batch = opts.batch ?? 5000;
  const now = opts.now ?? Date.now;
  // The transport gives up on signing after 30 seconds; stop before that.
  const deadline = now() + (opts.maxMs ?? 25_000);
  const base = ev.tags.filter((t) => t[0] !== 'nonce');
  const target = String(bits);
  for (let nonce = 0; ; nonce++) {
    const candidate = { ...ev, tags: [...base, ['nonce', String(nonce), target]] };
    if (leadingZeroBits(eventId(pubkey, candidate)) >= bits) return candidate;
    if ((nonce + 1) % batch === 0) {
      if (now() > deadline) throw new PowError('Proof of work took too long on this device. Try again.');
      opts.onProgress?.(nonce + 1);
      await new Promise((resolve) => setTimeout(resolve, 0));
    }
  }
}

/**
 * Finds the work for one event: returns `ev` with a NIP-13 `nonce` tag that
 * gives its id at least `bits` leading zero bits. Kind, content and the other
 * tags must stay as they are; `created_at` may change. `mine` is the built-in
 * one; `notemineMiner` from `@deadhop/client/notemine` is a faster one.
 */
export type Miner = (pubkey: string, ev: UnsignedEvent, bits: number, opts: MineOptions) => Promise<UnsignedEvent>;

const sameTags = (a: string[][], b: string[][]): boolean => JSON.stringify(a) === JSON.stringify(b);

/**
 * Throws unless `mined` is `ev` with at least `bits` of work: same kind and
 * content, the same tags apart from one `nonce` tag committing to `bits` or
 * more, and the original `created_at` or one within a minute of `nowSecs`.
 */
export function checkWork(pubkey: string, ev: UnsignedEvent, mined: UnsignedEvent, bits: number, nowSecs: number): void {
  const nonces = mined.tags.filter((t) => t[0] === 'nonce');
  const ok =
    mined.kind === ev.kind &&
    mined.content === ev.content &&
    sameTags(
      mined.tags.filter((t) => t[0] !== 'nonce'),
      ev.tags.filter((t) => t[0] !== 'nonce'),
    ) &&
    nonces.length === 1 &&
    Number(nonces[0][2]) >= bits &&
    (mined.created_at === ev.created_at || (Number.isInteger(mined.created_at) && Math.abs(mined.created_at - nowSecs) <= 60)) &&
    leadingZeroBits(eventId(pubkey, mined)) >= bits;
  if (!ok) throw new PowError('The miner returned an event without the required work.');
}

/** Only invoice creation costs work; everything else is sent as-is. */
export function needsWork(ev: UnsignedEvent): boolean {
  if (ev.kind !== CTXVM_KIND) return false;
  try {
    const msg = JSON.parse(ev.content);
    return msg?.method === 'tools/call' && msg?.params?.name === 'create_invoice';
  } catch {
    return false;
  }
}

/**
 * Wrap a signer so that invoice-creation requests carry the work the service
 * currently demands. `bits()` is read at signing time; `miner` does the work.
 */
export function powSigner(inner: NostrSigner, bits: () => number, opts: MineOptions = {}, miner: Miner = mine): NostrSigner {
  const now = opts.now ?? Date.now;
  return {
    getPublicKey: () => inner.getPublicKey(),
    nip44: inner.nip44,
    async signEvent(template) {
      const ev = template as UnsignedEvent;
      const required = bits();
      if (required <= 0 || !needsWork(ev)) return inner.signEvent(template);
      const pubkey = await inner.getPublicKey();
      const mined = await miner(pubkey, ev, required, opts);
      // Whatever mined it, never sign (and send) an event the service would refuse.
      checkWork(pubkey, ev, mined, required, Math.floor(now() / 1000));
      return inner.signEvent(mined as typeof template);
    },
  };
}
