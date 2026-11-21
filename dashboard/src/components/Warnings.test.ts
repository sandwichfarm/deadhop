import { afterEach, describe, expect, it } from 'vitest';
import { fireEvent, render, screen } from '@testing-library/svelte';
import Warnings from './Warnings.svelte';
import { loadDismissed } from '../lib/dismissed';

afterEach(() => localStorage.clear());

describe('Warnings', () => {
  it('can be dismissed, stay dismissed in this browser, and be shown again', async () => {
    const view = render(Warnings, { props: { warnings: ['electrum is remote', 'wallet not backed up'] } });
    expect(screen.getAllByRole('status')).toHaveLength(2);
    await fireEvent.click(screen.getByRole('button', { name: 'Dismiss: electrum is remote' }));
    expect(screen.queryByText('electrum is remote')).toBeNull();
    expect(screen.getByRole('button', { name: 'Show 1 dismissed warning' })).toBeTruthy();
    expect(loadDismissed()).toEqual(['electrum is remote']);
    view.unmount();

    // A reload keeps it hidden; a warning with other text still shows.
    render(Warnings, { props: { warnings: ['electrum is remote', 'wallet not backed up', 'new one'] } });
    expect(screen.queryByText('electrum is remote')).toBeNull();
    await fireEvent.click(screen.getByRole('button', { name: 'Dismiss: new one' }));
    await fireEvent.click(screen.getByRole('button', { name: 'Show 2 dismissed warnings' }));
    expect(screen.getAllByRole('status')).toHaveLength(3);
    expect(screen.queryByRole('button', { name: /^Show/ })).toBeNull();
    expect(loadDismissed()).toEqual([]);
  });
});
