// The Svelte 4 components on the real Svelte 4 runtime, through the same
// scenarios as every other framework.
import { describe, expect, it } from 'vitest';
import { VERSION } from 'svelte/compiler';
import * as kit from '../src/svelte4/index.js';
import { scenarios } from '../../shared/test/svelte-scenarios.ts';

describe('runtime', () => {
  it('is Svelte 4', () => {
    expect(VERSION.startsWith('4.')).toBe(true);
    // Svelte 4 compiles components to classes with $set; Svelte 5 to functions.
    expect(typeof kit.PayButton.prototype.$set).toBe('function');
  });
});

scenarios('Svelte 4 runtime', kit);
