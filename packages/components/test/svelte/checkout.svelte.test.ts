// The Svelte 5 headless binding on its own.
import { describe, expect, it } from 'vitest';
import { flushSync } from 'svelte';
import { useCheckout } from '../../src/svelte5/checkout.svelte.ts';
import { fakeClient, invoice } from '../fixtures.ts';

describe('useCheckout (Svelte 5)', () => {
  it('works without event callbacks', async () => {
    let c: ReturnType<typeof useCheckout> | undefined;
    const cleanup = $effect.root(() => {
      c = useCheckout(fakeClient([invoice()]));
    });
    expect(c!.state.phase).toBe('idle');
    await c!.checkout.watch(invoice().id);
    flushSync();
    expect(c!.state.phase).toBe('pending');
    cleanup();
  });
});
