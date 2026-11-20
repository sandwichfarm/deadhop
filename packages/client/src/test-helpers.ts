// Fixtures shared by the unit tests.
import type { Invoice, ServiceInfo } from './schemas.ts';

export const ADDRESS = 'bcrt1qw508d6qejxtdg4y5r3zarvary0c5xw7kygt080';
export const PUBKEY = '79be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798';

export function invoice(over: Partial<Invoice> = {}): Invoice {
  const base: Invoice = {
    id: '0123456789abcdef0123456789abcdef',
    address: ADDRESS,
    amount_sat: 50_000,
    memo: 'table 4',
    status: 'pending',
    received_sat: 0,
    confirmed_sat: 0,
    created_at: 1_700_000_000,
    expires_at: 1_700_003_600,
    settled_at: null,
    bip21: `bitcoin:${ADDRESS}?amount=0.00050000`,
    confirmations_required: 1,
  };
  return { ...base, ...over };
}

export function info(over: Partial<ServiceInfo> = {}): ServiceInfo {
  return {
    name: 'deadhop',
    version: '0.1.0',
    network: 'regtest',
    public_invoicing: true,
    pow_bits: 4,
    pow_window_secs: 600,
    min_confirmations: 1,
    default_expiry_secs: 3600,
    max_expiry_secs: 86400,
    min_amount_sat: 1000,
    max_amount_sat: 100_000_000,
    is_admin: false,
    ...over,
  };
}

/** Wrap a value the way an MCP tool result carries it. */
export function toolResult(structuredContent: unknown): { content: never[]; structuredContent: unknown; isError: boolean } {
  return { content: [], structuredContent, isError: false };
}
