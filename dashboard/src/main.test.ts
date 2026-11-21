import { afterEach, describe, expect, it, vi } from 'vitest';
import { screen, waitFor } from '@testing-library/svelte';

afterEach(() => {
  vi.resetModules();
  vi.unstubAllGlobals();
});

describe('entry point', () => {
  it('mounts the app into the page', async () => {
    document.body.innerHTML = '<div id="app"></div>';
    vi.spyOn(globalThis, 'fetch').mockRejectedValue(new Error('offline'));
    await import('./main');
    expect(document.querySelector('#app main')).not.toBeNull();
    await waitFor(() => expect(screen.getByRole('alert').textContent).toContain('config.json is missing'));
  });

  it('refuses to run inside a frame', async () => {
    document.body.innerHTML = '<div id="app"></div>';
    vi.stubGlobal('top', {});
    await import('./main');
    expect(document.querySelector('#app')!.textContent).toBe('This page cannot be shown inside another site.');
    expect(document.querySelector('#app main')).toBeNull();
  });
});
