import { describe, expect, it, vi } from 'vitest';
import { fireEvent, render, screen } from '@testing-library/svelte';
import Admin from './Admin.svelte';
import type { PayApi } from '../lib/api';
import { PUBKEY, adminInvoice, audit, health, relayHistory, info, invoice, metrics, series, settingsView, stats, privacyAudit, proxyCheck } from '../test-helpers';

describe('Admin', () => {
  it('switches between the operator views', async () => {
    const api = {
      getHealth: vi.fn().mockResolvedValue(health()),
      getRelayHistory: vi.fn().mockResolvedValue(relayHistory()),
      checkRelays: vi.fn().mockResolvedValue({ relays: [] }),
      getMetrics: vi.fn().mockResolvedValue(metrics()),
      getSeries: vi.fn().mockResolvedValue(series()),
      getStats: vi.fn().mockResolvedValue(stats()),
      listInvoices: vi
        .fn()
        .mockResolvedValueOnce({ invoices: [adminInvoice()], next_before: null })
        .mockResolvedValue({ invoices: [], next_before: null }),
      createInvoice: vi.fn().mockResolvedValue(invoice()),
      getSettings: vi.fn().mockResolvedValue(settingsView()),
      getAuditLog: vi.fn().mockResolvedValue(audit()),
      listBackups: vi.fn().mockResolvedValue({ backups: [] }),
      getPrivacyAudit: vi.fn().mockResolvedValue(privacyAudit()),
      checkProxy: vi.fn().mockResolvedValue(proxyCheck()),
    };
    render(Admin, {
      props: { api: api as unknown as PayApi, info: info({ is_admin: true }), onCreated: vi.fn(), dashboardRelays: ['ws://127.0.0.1:7777'], serverPubkey: PUBKEY, onSettingsChanged: vi.fn() },
    });
    await screen.findByText('Addresses created');
    expect(screen.getByRole('region', { name: 'Service key' })).toBeTruthy();
    expect(screen.getByRole('button', { name: 'Overview' }).getAttribute('aria-current')).toBe('page');

    // "All invoices" under the recent drops opens the Invoices tab.
    await fireEvent.click(await screen.findByRole('button', { name: 'All invoices' }));
    await screen.findByText('No invoices yet.');
    expect(screen.getByRole('button', { name: 'Invoices' }).getAttribute('aria-current')).toBe('page');
    expect(screen.getByRole('button', { name: 'Overview' }).getAttribute('aria-current')).toBeNull();

    await fireEvent.click(screen.getByRole('button', { name: 'Settings' }));
    await screen.findByRole('heading', { name: 'Relays and proxy' });

    await fireEvent.click(screen.getByRole('button', { name: 'Operators' }));
    await screen.findByRole('heading', { name: 'Audit log' });
    expect(screen.getByRole('heading', { name: 'Who can run this service' })).toBeTruthy();

    await fireEvent.click(screen.getByRole('button', { name: 'Maintenance' }));
    await screen.findByRole('heading', { name: 'Wipe invoices' });

    // The privacy score opens the audit, which belongs to the Overview tab.
    await fireEvent.click(screen.getByRole('button', { name: 'Overview' }));
    await fireEvent.click(await screen.findByRole('button', { name: /^Privacy score/ }));
    await screen.findByRole('heading', { name: 'Network location' });
    expect(screen.getByRole('button', { name: 'Overview' }).getAttribute('aria-current')).toBe('page');
    await fireEvent.click(screen.getByRole('button', { name: '← Overview' }));
    await screen.findByText('Addresses created');

    // The Connectivity page belongs to the Overview tab, and leads back to it.
    await fireEvent.click(screen.getByRole('button', { name: 'Connectivity' }));
    await screen.findByRole('heading', { level: 1, name: 'Relays' });
    expect(screen.getByRole('button', { name: 'Overview' }).getAttribute('aria-current')).toBe('page');
    await fireEvent.click(screen.getByRole('button', { name: '← Overview' }));
    await screen.findByText('Addresses created');
  });
});
