import { describe, expect, it } from 'vitest';
import { render, screen } from '@testing-library/svelte';
import Chart from './Chart.svelte';
import { series } from '../test-helpers';

describe('Chart', () => {
  it('draws a line, marks offline time and has a table', () => {
    const { container } = render(Chart, { props: { series: series() } });
    expect(container.querySelector('path.line')?.getAttribute('d')).toMatch(/^M8\.0,172\.0 L/);
    expect(container.querySelectorAll('circle.dot')).toHaveLength(1);
    expect(container.querySelector('rect.offline')).toBeTruthy();
    expect(screen.getByText('peak 100,000 sats')).toBeTruthy();
    expect(screen.getAllByRole('row')).toHaveLength(8);
  });

  it('copes with an empty series, a single bucket and counts', () => {
    const empty = series({ online_since: 0, points: [{ start: 1_700_000_000, count: 0, sat: 0 }] });
    const { container } = render(Chart, { props: { series: empty, metric: 'count' } });
    expect(container.querySelector('rect.offline')).toBeNull();
    expect(container.querySelectorAll('circle.dot')).toHaveLength(0);
    expect(screen.getByText('peak 0 payments')).toBeTruthy();
    expect(screen.getByRole('img').getAttribute('aria-label')).toContain('0 payments in 1 buckets');
    document.body.innerHTML = '';
    render(Chart, { props: { series: series(), metric: 'count' } });
    expect(screen.getByText('peak 2 payments')).toBeTruthy();
    document.body.innerHTML = '';
    render(Chart, { props: { series: series({ points: [{ start: 1, count: 1, sat: 1 }, { start: 2, count: 0, sat: 0 }] }), metric: 'count' } });
    expect(screen.getByText('peak 1 payment')).toBeTruthy();
  });
});
