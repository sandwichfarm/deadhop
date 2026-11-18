import { describe, expect, it, vi } from 'vitest';
import { fireEvent, render, screen } from '@testing-library/svelte';
import Login from './Login.svelte';

describe('Login', () => {
  it('offers extension, secret key and guest sign-in', async () => {
    const onSignIn = vi.fn();
    render(Login, { props: { onSignIn } });

    await fireEvent.click(screen.getByRole('button', { name: 'Use browser extension' }));
    expect(onSignIn).toHaveBeenLastCalledWith('extension');

    await fireEvent.click(screen.getByRole('button', { name: 'Continue as guest' }));
    expect(onSignIn).toHaveBeenLastCalledWith('guest');

    expect(screen.getByText(/never enters this page/)).toBeTruthy();
    const input = screen.getByLabelText(/paste a secret key/) as HTMLInputElement;
    const submit = screen.getByRole('button', { name: 'Sign in' }) as HTMLButtonElement;
    expect(input.type).toBe('password');
    expect(submit.disabled).toBe(true);
    await fireEvent.input(input, { target: { value: 'nsec1abc' } });
    expect(submit.disabled).toBe(false);
    await fireEvent.submit(input.closest('form')!);
    expect(onSignIn).toHaveBeenLastCalledWith('secret', 'nsec1abc');
    // The key is not left sitting in the form.
    expect(input.value).toBe('');
  });

  it('shows progress and errors, and locks the buttons while busy', () => {
    render(Login, { props: { onSignIn: vi.fn(), busy: true, error: 'Could not reach the service.' } });
    expect(screen.getByRole('status').textContent).toBe('Connecting…');
    expect(screen.getByRole('alert').textContent).toBe('Could not reach the service.');
    for (const b of screen.getAllByRole('button')) expect((b as HTMLButtonElement).disabled).toBe(true);
  });
});
