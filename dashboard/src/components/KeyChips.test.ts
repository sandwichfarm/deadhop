import { describe, expect, it, vi } from 'vitest';
import { fireEvent, render, screen } from '@testing-library/svelte';
import KeyChips from './KeyChips.svelte';
import KeyBadges from './KeyBadges.svelte';
import { keyInfos } from '../test-helpers';

describe('KeyChips', () => {
  it('shows one switch per key, all on for an empty assignment, and reports a change', async () => {
    const onchange = vi.fn();
    render(KeyChips, { props: { keys: keyInfos(), assigned: [], label: 'Relay', onchange } });
    expect((screen.getByLabelText('Relay used by Administration') as HTMLInputElement).checked).toBe(true);
    expect(screen.getByText('all keys')).toBeTruthy();
    await fireEvent.click(screen.getByLabelText('Relay used by Administration'));
    expect(onchange).toHaveBeenCalledWith('admin', false);
  });

  it('keeps the last key switched on and can be disabled as a whole', () => {
    render(KeyChips, { props: { keys: keyInfos(), assigned: ['invoices'], label: 'Relay', onchange: vi.fn() } });
    expect((screen.getByLabelText('Relay used by Public invoices') as HTMLInputElement).disabled).toBe(true);
    expect((screen.getByLabelText('Relay used by Administration') as HTMLInputElement).disabled).toBe(false);
    expect(screen.queryByText('all keys')).toBeNull();
    document.body.innerHTML = '';
    render(KeyChips, { props: { keys: keyInfos(), assigned: [], label: 'Relay', onchange: vi.fn(), disabled: true } });
    expect((screen.getByLabelText('Relay used by Administration') as HTMLInputElement).disabled).toBe(true);
  });

  it('is not shown while there is only one key', () => {
    render(KeyChips, { props: { keys: keyInfos().slice(0, 1), assigned: [], label: 'Relay', onchange: vi.fn() } });
    expect(screen.queryByRole('group')).toBeNull();
  });
});

describe('KeyBadges', () => {
  it('names the keys only when not every key uses the thing', () => {
    const { container, rerender } = render(KeyBadges, { props: { keys: ['invoices'], total: 2 } });
    expect(container.textContent).toBe('Invoices');
    rerender({ keys: ['invoices', 'admin'], total: 2 });
    expect(container.textContent).toBe('');
    rerender({ keys: [], total: 2 });
    expect(container.textContent).toBe('');
  });
});
