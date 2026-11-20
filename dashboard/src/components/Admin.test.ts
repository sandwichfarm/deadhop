import { describe, expect, it, vi } from 'vitest';
import { fireEvent, render, screen } from '@testing-library/svelte';
import Admin from './Admin.svelte';
import type { PayApi } from '../lib/api';
import { PUBKEY, audit, health, info, invoice, metrics, series, settingsView, stats, privacyAudit, proxyCheck } from '../test-helpers';

describe('Admin', () => {
  it('switches between the operator views', async () => {
    const api = {
      getHealth: vi.fn().mockResolvedValue(health()),
      getMetrics: vi.fn().mockResolvedValue(metrics()),
      getSeries: vi.fn().mockResolvedValue(series()),
      getStats: vi.fn().mockResolvedValue(stats()),
      listInvoices: vi.fn().mockResolvedValue({ invoices: [], next_before: null }),
      createInvoice: vi.fn().mockResolvedValue(invoice()),
      getSettings: vi.fn().mockResolvedValue(settingsView()),
      getAuditLog: vi.fn().mockResolvedValue(audit()),
      getPrivacyAudit: vi.fn().mockResolvedValue(privacyAudit()),
      checkProxy: vi.fn().mockResolvedValue(proxyCheck()),
    };
    render(Admin, {
      props: { api: api as unknown as PayApi, info: info({ is_admin: true }), onCreated: vi.fn(), dashboardRelays: ['ws://127.0.0.1:7777'], serverPubkey: PUBKEY, onSettingsChanged: vi.fn() },
    });
    await screen.findByText('Addresses created');
    expect(screen.getByRole('heading', { name: 'Service key' })).toBeTruthy();
    expect(screen.getByRole('button', { name: 'Overview' }).getAttribute('aria-current')).toBe('page');

    await fireEvent.click(screen.getByRole('button', { name: 'Invoices' }));
    await screen.findByText('No invoices yet.');
    expect(screen.getByRole('button', { name: 'Overview' }).getAttribute('aria-current')).toBeNull();

    await fireEvent.click(screen.getByRole('button', { name: 'Settings' }));
    await screen.findByRole('heading', { name: 'Operators' });

    await fireEvent.click(screen.getByRole('button', { name: 'Maintenance' }));
    await screen.findByRole('heading', { name: 'Audit log' });

    // The privacy score opens the audit, which belongs to the Overview tab.
    await fireEvent.click(screen.getByRole('button', { name: 'Overview' }));
    await fireEvent.click(await screen.findByRole('button', { name: /^Privacy score/ }));
    await screen.findByRole('heading', { name: 'Network location' });
    expect(screen.getByRole('button', { name: 'Overview' }).getAttribute('aria-current')).toBe('page');
    await fireEvent.click(screen.getByRole('button', { name: '← Overview' }));
    await screen.findByText('Addresses created');
  });
});
