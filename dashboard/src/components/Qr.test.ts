import { describe, expect, it } from 'vitest';
import { render, screen } from '@testing-library/svelte';
import Qr from './Qr.svelte';

describe('Qr', () => {
  it('renders a labelled SVG code', () => {
    render(Qr, { props: { text: 'bitcoin:bcrt1qexample?amount=0.00050000', label: 'Payment QR code' } });
    const svg = screen.getByRole('img', { name: 'Payment QR code' });
    expect(svg.querySelector('path')!.getAttribute('d')).toMatch(/^M\d+ \d+h1v1h-1z/);
    expect(svg.getAttribute('viewBox')).toMatch(/^0 0 \d+ \d+$/);
  });
});
