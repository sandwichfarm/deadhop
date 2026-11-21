import { afterEach, describe, expect, it } from 'vitest';
import { loadDismissed, saveDismissed } from './dismissed';

afterEach(() => localStorage.clear());

describe('dismissed warnings', () => {
  it('are kept in local storage', () => {
    expect(loadDismissed()).toEqual([]);
    saveDismissed(['a', 'b']);
    expect(loadDismissed()).toEqual(['a', 'b']);
  });

  it('ignore anything stored that is not a list of strings', () => {
    localStorage.setItem('deadhop.dismissed-warnings', '{not json');
    expect(loadDismissed()).toEqual([]);
    localStorage.setItem('deadhop.dismissed-warnings', '{"a":1}');
    expect(loadDismissed()).toEqual([]);
    localStorage.setItem('deadhop.dismissed-warnings', '["a",2,null]');
    expect(loadDismissed()).toEqual(['a']);
  });

  it('survive a storage that refuses writes', () => {
    const full = { getItem: () => null, setItem: () => { throw new Error('quota'); } } as unknown as Storage;
    expect(() => saveDismissed(['a'], full)).not.toThrow();
  });
});
