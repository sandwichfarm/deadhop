/**
 * Faster proof of work with notemine (https://github.com/sandwichfarm/notemine):
 * WebAssembly in several Web Workers instead of the built-in miner on the main
 * thread. Opt in by passing the miner to `createDeadhop`:
 *
 * ```ts
 * import { createDeadhop } from '@deadhop/client';
 * import { notemineMiner } from '@deadhop/client/notemine';
 *
 * const deadhop = createDeadhop({ serverPubkey, relays, miner: notemineMiner() });
 * ```
 *
 * Needs `npm install @notemine/wrapper` (an optional peer dependency) and, if
 * the page has a Content-Security-Policy, `worker-src blob:` and
 * `script-src 'wasm-unsafe-eval'`. Where notemine cannot run (no Web Workers,
 * a stricter policy, a server), the built-in miner takes over.
 *
 * @module
 */
import { checkWork, mine, PowError, type MineOptions, type Miner, type UnsignedEvent } from './pow.ts';

interface Subscribable<T> {
  subscribe(next: (value: T) => void): { unsubscribe(): void };
}

/** What this adapter passes to notemine's `Notemine` constructor. */
export interface NotemineSettings {
  pubkey: string;
  kind: number;
  tags: string[][];
  content: string;
  difficulty: number;
  numberOfWorkers: number;
}

/** The part of notemine's `Notemine` class this adapter uses. */
export interface NotemineInstance {
  mine(): Promise<void>;
  cancel(): void;
  success$: Subscribable<{ result: { event: unknown } | null }>;
  error$: Subscribable<{ error: unknown }>;
}

/** notemine's `Notemine` class, or anything with the same shape. */
export type NotemineClass = new (settings: NotemineSettings) => NotemineInstance;

/** Options for `notemineMiner`. */
export interface NotemineMinerOptions {
  /** Web Workers to mine with (default: one less than the device's cores, 1 to 8). */
  workers?: number;
  /**
   * Mines when notemine cannot run here. Default: the built-in miner. `false`
   * makes such a failure an error instead.
   */
  fallback?: Miner | false;
  /**
   * Loads notemine. Default: `import('@notemine/wrapper')`, so its code (about
   * 500 KB) is fetched only when the first invoice needs work.
   */
  load?: () => Promise<{ Notemine: NotemineClass }>;
}

/** Raised (internally) when mining ran out of time, which a fallback would not fix. */
class OutOfTime extends PowError {}

function defaultWorkers(): number {
  const cores = globalThis.navigator?.hardwareConcurrency || 2;
  return Math.max(1, Math.min(8, cores - 1));
}

const loadNotemine = (): Promise<{ Notemine: NotemineClass }> =>
  import('@notemine/wrapper') as unknown as Promise<{ Notemine: NotemineClass }>;

/** The event notemine returns, as the unsigned event the signer needs. */
function unsigned(event: unknown): UnsignedEvent {
  const e = (typeof event === 'string' ? JSON.parse(event) : event) as UnsignedEvent;
  return { kind: e.kind, tags: e.tags, content: e.content, created_at: e.created_at };
}

/** Mine once with notemine, within `ms`. */
function run(Notemine: NotemineClass, settings: NotemineSettings, ms: number): Promise<UnsignedEvent> {
  const miner = new Notemine(settings);
  return new Promise<UnsignedEvent>((resolve, reject) => {
    const subs: { unsubscribe(): void }[] = [];
    let timer: ReturnType<typeof setTimeout> | undefined;
    const settle = (fn: () => void) => {
      clearTimeout(timer);
      for (const s of subs) s.unsubscribe();
      fn();
    };
    const fail = (e: unknown) => {
      miner.cancel();
      settle(() => reject(e));
    };
    timer = setTimeout(() => fail(new OutOfTime('Proof of work took too long on this device. Try again.')), ms);
    subs.push(
      miner.success$.subscribe(({ result }) => {
        if (!result) return fail(new PowError('notemine finished without a result.'));
        try {
          const ev = unsigned(result.event);
          settle(() => resolve(ev));
        } catch (e) {
          fail(e);
        }
      }),
      miner.error$.subscribe(({ error }) => fail(new PowError(`notemine failed: ${String(error)}`))),
    );
    miner.mine().catch(fail);
  });
}

/**
 * A `Miner` that uses notemine. Notemine is loaded on first use; if it cannot
 * run on this page it is not tried again, and `fallback` mines instead.
 */
export function notemineMiner(options: NotemineMinerOptions = {}): Miner {
  const load = options.load ?? loadNotemine;
  const fallback = options.fallback ?? mine;
  let broken = false;

  return async (pubkey: string, ev: UnsignedEvent, bits: number, opts: MineOptions): Promise<UnsignedEvent> => {
    const now = opts.now ?? Date.now;
    const started = now();
    const budget = opts.maxMs ?? 25_000;
    try {
      if (broken || typeof Worker === 'undefined') throw new PowError('notemine cannot run here.');
      const { Notemine } = await load();
      const tags = ev.tags.filter((t) => t[0] !== 'nonce');
      const settings = { pubkey, kind: ev.kind, tags, content: ev.content, difficulty: bits, numberOfWorkers: options.workers ?? defaultWorkers() };
      const mined = await run(Notemine, settings, budget);
      checkWork(pubkey, ev, mined, bits, Math.floor(now() / 1000));
      return mined;
    } catch (e) {
      if (e instanceof OutOfTime) throw new PowError(e.message);
      broken = true;
      if (fallback === false) throw e instanceof PowError ? e : new PowError(`notemine failed: ${String(e)}`);
      return fallback(pubkey, ev, bits, { ...opts, maxMs: Math.max(0, budget - (now() - started)) });
    }
  };
}
