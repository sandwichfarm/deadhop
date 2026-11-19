import { describe, expect, it } from 'vitest';
import { qrShape } from './qr.ts';
import { ADDRESS } from './test-helpers.ts';

describe('qrShape', () => {
  it('produces a square path made only of module rectangles', () => {
    const { size, path } = qrShape(`bitcoin:${ADDRESS}?amount=0.00050000`);
    expect(size).toBeGreaterThan(21);
    expect(path).toMatch(/^(M\d+ \d+h1v1h-1z)+$/);
    // Finder pattern: the first dark module sits just inside the quiet zone.
    expect(path.startsWith('M2 2h1v1h-1z')).toBe(true);
  });

  it('differs for different content', () => {
    expect(qrShape('a').path).not.toBe(qrShape('b').path);
  });
});
