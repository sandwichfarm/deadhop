import { describe, expect, it, vi } from 'vitest';
import { fireEvent, render, screen } from '@testing-library/svelte';
import PrivacyAudit from './PrivacyAudit.svelte';
import type { PayApi } from '../lib/api';
import { privacyAudit } from '../test-helpers';

function setup(getPrivacyAudit = vi.fn().mockResolvedValue(privacyAudit())) {
  const onBack = vi.fn();
  render(PrivacyAudit, { props: { api: { getPrivacyAudit } as unknown as PayApi, onBack } });
  return { getPrivacyAudit, onBack };
}

describe('PrivacyAudit', () => {
  it('shows the score, the counts and every check by area', async () => {
    setup();
    expect(await screen.findByRole('img', { name: 'Privacy score 90 out of 100' })).toBeTruthy();
    expect(screen.getByRole('img').getAttribute('class')).toContain('ok');
    expect(screen.getByText(/8 of 10 checked/)).toBeTruthy();
    expect(screen.getByRole('heading', { name: '1 needs attention' })).toBeTruthy();
    expect(screen.getByText('2 pass')).toBeTruthy();
    expect(screen.getByText('1 not checked')).toBeTruthy();
    for (const area of ['Network location', 'Open ports', 'Exposure']) {
      expect(screen.getByRole('heading', { name: area })).toBeTruthy();
    }
    // What needs fixing is open, with its evidence, points and fix.
    const lock = screen.getByText('The proxy can be changed remotely').closest('details')!;
    expect(lock.open).toBe(true);
    expect(lock.textContent).toContain('DEADHOP_PROXY_LOCKED   false');
    expect(lock.textContent).toContain('−10');
    expect(lock.textContent).toContain('Set DEADHOP_PROXY_LOCKED=true on the host.');
    expect(screen.getByText('Your IP is hidden from relays').closest('details')!.open).toBe(false);
  });

  it('says when nothing needs fixing, colours low scores, runs again and goes back', async () => {
    const t = setup(
      vi.fn()
        .mockResolvedValueOnce(privacyAudit({ score: 70, checks: [] }))
        .mockResolvedValueOnce(privacyAudit({ score: 40, checks: privacyAudit().checks.map((c) => ({ ...c, result: 'fail' as const })) })),
    );
    expect(await screen.findByRole('heading', { name: 'Nothing needs attention' })).toBeTruthy();
    expect(screen.getByRole('img').getAttribute('class')).toContain('warn');
    expect(screen.queryByRole('heading', { name: 'Network location' })).toBeNull();
    await fireEvent.click(screen.getByRole('button', { name: 'Run again' }));
    expect(await screen.findByRole('img', { name: 'Privacy score 40 out of 100' })).toBeTruthy();
    expect(screen.getByRole('img').getAttribute('class')).toContain('bad');
    expect(screen.getByRole('heading', { name: '4 need attention' })).toBeTruthy();
    expect(t.getPrivacyAudit).toHaveBeenCalledTimes(2);
    await fireEvent.click(screen.getByRole('button', { name: '← Overview' }));
    expect(t.onBack).toHaveBeenCalled();
  });

  it('shows why the audit could not run', async () => {
    setup(vi.fn().mockRejectedValue(new Error('unauthorized')));
    expect((await screen.findByRole('alert')).textContent).toBe('unauthorized');
  });
});
