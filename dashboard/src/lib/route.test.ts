import { describe, expect, it } from 'vitest';
import { parseRoute, payHash } from './route';

describe('routes', () => {
  const id = '0123456789abcdef0123456789abcdef';

  it('parses payment links', () => {
    expect(parseRoute(payHash(id))).toEqual({ name: 'pay', id });
  });

  it('treats everything else as home', () => {
    for (const hash of ['', '#/', '#/pay/', '#/pay/xyz', `#/pay/${id}/extra`, `#/pay/${id.toUpperCase()}`, '#/pay/<script>']) {
      expect(parseRoute(hash), hash).toEqual({ name: 'home' });
    }
  });
});
