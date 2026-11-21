import { describe, expect, it } from 'vitest';
import { render, screen } from '@testing-library/svelte';
import RelayBars from './RelayBars.svelte';

describe('RelayBars', () => {
  it('stacks failures under what worked, keeps a lone failure visible, and marks empty buckets', () => {
    const points = [
      { start: 1_700_000_000, ok: 100, fail: 0 },
      { start: 1_700_003_600, ok: 99, fail: 1 },
      { start: 1_700_007_200, ok: 0, fail: 0 },
      { start: 1_700_010_800, ok: 0, fail: 50 },
    ];
    render(RelayBars, { props: { points, label: 'wss://a.example' } });
    const svg = screen.getByRole('img');
    expect(svg.getAttribute('aria-label')).toBe('wss://a.example: 51 of 250 results failed, from 2023-11-14 22:13 UTC');
    expect(svg.getAttribute('class')).toBe('bars');
    expect(svg.getAttribute('viewBox')).toBe('0 0 40 40');
    const heights = (cls: string) => [...svg.querySelectorAll(`rect.${cls}`)].map((r) => Number(Number(r.getAttribute('height')).toFixed(1)));
    expect(heights('bar-ok')).toEqual([40, 39.6]);
    expect(heights('bar-fail')).toEqual([2, 20]);
    expect(svg.querySelectorAll('rect.bar-none')).toHaveLength(1);
  });

  it('can be drawn tall', () => {
    render(RelayBars, { props: { points: [{ start: 1_700_000_000, ok: 1, fail: 0 }], label: 'x', tall: true } });
    expect(screen.getByRole('img').getAttribute('class')).toBe('bars tall');
  });
});
