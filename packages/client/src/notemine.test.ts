import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { CTXVM_KIND, PowError, eventId, leadingZeroBits, mine, type MineOptions, type Miner, type UnsignedEvent } from './pow.ts';
import { notemineMiner, type NotemineClass, type NotemineInstance, type NotemineSettings } from './notemine.ts';
import { PUBKEY } from './test-helpers.ts';

// The default loader imports @notemine/wrapper; it is replaced by the fake below.
vi.mock('@notemine/wrapper', () => ({ Notemine: Fake }));

const createCall = JSON.stringify({ jsonrpc: '2.0', id: 1, method: 'tools/call', params: { name: 'create_invoice' } });
const NOW_MS = 1_700_000_000_000;
const template = (): UnsignedEvent => ({ kind: CTXVM_KIND, tags: [['p', PUBKEY], ['nonce', '9', '1']], content: createCall, created_at: NOW_MS / 1000 });
const opts: MineOptions = { now: () => NOW_MS };

class Stream<T> {
  subs = new Set<(v: T) => void>();
  subscribe(next: (v: T) => void) {
    this.subs.add(next);
    return { unsubscribe: () => this.subs.delete(next) };
  }
  next(v: T) {
    for (const s of [...this.subs]) s(v);
  }
}

type Mode = 'object' | 'string' | 'null' | 'garbage' | 'error' | 'reject' | 'hang' | 'lazy';
let mode: Mode = 'object';
const made: Fake[] = [];

/** Stands in for notemine: mines with the built-in miner and reports like notemine does. */
class Fake implements NotemineInstance {
  success$ = new Stream<{ result: { event: unknown } | null }>();
  error$ = new Stream<{ error: unknown }>();
  cancel = vi.fn();
  constructor(readonly settings: NotemineSettings) {
    made.push(this);
  }
  async mine() {
    const s = this.settings;
    if (mode === 'reject') throw new Error('no wasm');
    if (mode === 'hang') return;
    if (mode === 'error') return this.error$.next({ error: 'worker blew up' });
    if (mode === 'null') return this.success$.next({ result: null });
    if (mode === 'garbage') return this.success$.next({ result: { event: '{not json' } });
    const ev = { kind: s.kind, tags: s.tags, content: s.content, created_at: NOW_MS / 1000 + 2 };
    const mined = mode === 'lazy' ? ev : await mine(s.pubkey, ev, s.difficulty);
    const event = { ...mined, pubkey: s.pubkey, id: eventId(s.pubkey, mined), sig: 'x' };
    this.success$.next({ result: { event: mode === 'string' ? JSON.stringify(event) : event } });
  }
}

const load = async () => ({ Notemine: Fake as unknown as NotemineClass });
const spyMiner = () => vi.fn<Miner>((pk, ev, bits, o) => mine(pk, ev, bits, o));

beforeEach(() => {
  mode = 'object';
  made.length = 0;
  vi.stubGlobal('Worker', class {});
});
afterEach(() => vi.unstubAllGlobals());

describe('notemineMiner', () => {
  it('mines with notemine and returns an unsigned event with the work', async () => {
    const fallback = spyMiner();
    const miner = notemineMiner({ load, workers: 3, fallback });
    const mined = await miner(PUBKEY, template(), 8, opts);
    expect(leadingZeroBits(eventId(PUBKEY, mined))).toBeGreaterThanOrEqual(8);
    expect(Object.keys(mined).sort()).toEqual(['content', 'created_at', 'kind', 'tags']);
    expect(made[0].settings).toEqual({ pubkey: PUBKEY, kind: CTXVM_KIND, tags: [['p', PUBKEY]], content: createCall, difficulty: 8, numberOfWorkers: 3 });
    expect(fallback).not.toHaveBeenCalled();
    // A JSON string result works too.
    mode = 'string';
    expect(leadingZeroBits(eventId(PUBKEY, await miner(PUBKEY, template(), 8, opts)))).toBeGreaterThanOrEqual(8);
  });

  it('loads @notemine/wrapper by default and sizes the workers to the device', async () => {
    vi.stubGlobal('navigator', { hardwareConcurrency: 4 });
    await notemineMiner()(PUBKEY, template(), 4, opts);
    expect(made[0].settings.numberOfWorkers).toBe(3);
    vi.stubGlobal('navigator', { hardwareConcurrency: 64 });
    await notemineMiner({ load })(PUBKEY, template(), 4, opts);
    expect(made[1].settings.numberOfWorkers).toBe(8);
    vi.stubGlobal('navigator', undefined);
    await notemineMiner({ load })(PUBKEY, template(), 4, opts);
    expect(made[2].settings.numberOfWorkers).toBe(1);
  });

  it.each<Mode>(['error', 'reject', 'null', 'garbage', 'lazy'])('falls back to the built-in miner when notemine %s', async (m) => {
    mode = m;
    const fallback = spyMiner();
    const miner = notemineMiner({ load, fallback });
    const mined = await miner(PUBKEY, template(), 6, opts);
    expect(leadingZeroBits(eventId(PUBKEY, mined))).toBeGreaterThanOrEqual(6);
    expect(fallback).toHaveBeenCalledWith(PUBKEY, template(), 6, expect.objectContaining({ maxMs: 25_000 }));
    // Not tried again on this page.
    await miner(PUBKEY, template(), 6, opts);
    expect(made).toHaveLength(1);
    expect(fallback).toHaveBeenCalledTimes(2);
  });

  it('cancels notemine when it fails', async () => {
    mode = 'error';
    await notemineMiner({ load, fallback: spyMiner() })(PUBKEY, template(), 4, opts);
    expect(made[0].cancel).toHaveBeenCalled();
  });

  it('falls back without Web Workers (servers, old browsers) and when notemine is not installed', async () => {
    vi.stubGlobal('Worker', undefined);
    const fallback = spyMiner();
    await notemineMiner({ load, fallback })(PUBKEY, template(), 4, opts);
    expect(made).toHaveLength(0);
    vi.stubGlobal('Worker', class {});
    const missing = () => Promise.reject(new Error('Cannot find package @notemine/wrapper'));
    await notemineMiner({ load: missing, fallback })(PUBKEY, template(), 4, opts);
    expect(fallback).toHaveBeenCalledTimes(2);
  });

  it('gives the fallback only the time left', async () => {
    let t = NOW_MS;
    const slowLoad = async () => {
      t += 10_000;
      throw new Error('slow and broken');
    };
    const fallback = spyMiner();
    await notemineMiner({ load: slowLoad, fallback })(PUBKEY, template(), 4, { now: () => t, maxMs: 15_000 });
    expect(fallback.mock.calls[0][3].maxMs).toBe(5_000);
    t = NOW_MS;
    const slower = async () => {
      t += 20_000;
      throw new Error('slower');
    };
    await expect(notemineMiner({ load: slower })(PUBKEY, template(), 20, { now: () => t++, maxMs: 15_000, batch: 1 })).rejects.toThrow('took too long');
  });

  it('with fallback: false, a failure is an error', async () => {
    mode = 'error';
    await expect(notemineMiner({ load, fallback: false })(PUBKEY, template(), 4, opts)).rejects.toThrow('notemine failed: worker blew up');
    const missing = () => Promise.reject(new Error('gone'));
    const err = notemineMiner({ load: missing, fallback: false })(PUBKEY, template(), 4, opts);
    await expect(err).rejects.toBeInstanceOf(PowError);
    await expect(err).rejects.toThrow('notemine failed: Error: gone');
  });

  it('stops after the time limit without falling back', async () => {
    vi.useFakeTimers();
    try {
      mode = 'hang';
      const fallback = spyMiner();
      const miner = notemineMiner({ load, fallback });
      const result = miner(PUBKEY, template(), 4, { now: () => NOW_MS, maxMs: 1000 });
      const settled = expect(result).rejects.toThrow('took too long');
      await vi.advanceTimersByTimeAsync(1001);
      await settled;
      await expect(result).rejects.toBeInstanceOf(PowError);
      expect(made[0].cancel).toHaveBeenCalled();
      expect(fallback).not.toHaveBeenCalled();
      // A timeout does not mean notemine is broken: the next invoice tries it again.
      mode = 'object';
      vi.useRealTimers();
      await miner(PUBKEY, template(), 4, opts);
      expect(made).toHaveLength(2);
      expect(fallback).not.toHaveBeenCalled();
    } finally {
      vi.useRealTimers();
    }
  });

  it('uses the real clock by default', async () => {
    const real = { ...template(), created_at: Math.floor(Date.now() / 1000) };
    const mined = await notemineMiner({ load })(PUBKEY, real, 4, {});
    expect(leadingZeroBits(eventId(PUBKEY, mined))).toBeGreaterThanOrEqual(4);
  });
});
