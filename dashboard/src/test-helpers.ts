// Fixtures shared by the unit tests.
import type { AdminInvoice, AuditLog, Health, Info, Invoice, Metrics, PrivacyAudit, ProxyCheck, ProxyPool, RelayHistory, RelayStats, Series, Settings, SettingsView, Stats } from './lib/schemas';

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

export function adminInvoice(over: Partial<AdminInvoice> = {}): AdminInvoice {
  return { ...invoice(), index: 0, creator: PUBKEY, ...over };
}

export function info(over: Partial<Info> = {}): Info {
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

export function stats(over: Partial<Stats> = {}): Stats {
  return {
    pending: 2,
    detected: 1,
    confirmed: 3,
    expired: 4,
    confirmed_sat: 150_000,
    detected_sat: 50_000,
    next_index: 10,
    tip_height: 101,
    last_poll_at: 1_700_000_100,
    chain_ok: true,
    watch_errors: 0,
    ...over,
  };
}

/** Wrap a value the way an MCP tool result carries it. */
export function toolResult(structuredContent: unknown) {
  return { content: [], structuredContent, isError: false };
}

export function proxyCheck(over: Partial<ProxyCheck> = {}): ProxyCheck {
  return {
    proxy: 'socks5://127.0.0.1:9050',
    status: 'connected',
    checked_at: 1_700_000_050,
    relays: [{ relay: 'ws://127.0.0.1:7777', ok: true, code: 'SOCKS5 00', reason: 'Reached through the proxy.', ms: 84 }],
    ...over,
  };
}

export function privacyAudit(over: Partial<PrivacyAudit> = {}): PrivacyAudit {
  return {
    score: 90,
    checked: 8,
    total: 10,
    ran_at: 1_700_000_060,
    checks: [
      { id: 'relays_via_proxy', area: 'network', title: 'Your IP is hidden from relays', result: 'pass', points: 0, summary: 'Relay connections go through the proxy.', evidence: ['proxy   socks5://127.0.0.1:9050'], fix: null },
      { id: 'proxy_locked', area: 'network', title: 'The proxy can be changed remotely', result: 'to_fix', points: 10, summary: 'A stolen operator key could set a proxy it controls.', evidence: ['DEADHOP_PROXY_LOCKED   false'], fix: 'Set DEADHOP_PROXY_LOCKED=true on the host.' },
      { id: 'host_ports', area: 'ports', title: "The host's open ports", result: 'not_checked', points: 0, summary: 'The service sees only its own network namespace.', evidence: [], fix: 'On the host, list them with `ss -tlnp`.' },
      { id: 'announce', area: 'exposure', title: 'No public announcements', result: 'pass', points: 0, summary: 'The service publishes no discovery events.', evidence: ['DEADHOP_ANNOUNCE   false'], fix: null },
    ],
    ...over,
  };
}

export function proxyPool(over: Partial<ProxyPool> = {}): ProxyPool {
  return { state: 'direct', posture: 'normal', mode: 'failover', offline: 'kill_switch', transparent: false, active: null, reachable: 0, total: 0, next_switch_at: null, proxies: [], ...over };
}

/** One proxy in use, as the service reports it. */
export function activePool(over: Partial<ProxyPool> = {}): ProxyPool {
  return proxyPool({
    state: 'active',
    active: 'p1',
    reachable: 1,
    total: 1,
    proxies: [{ id: 'p1', name: '', url: 'socks5://127.0.0.1:9050', reachable: true, checked_at: 1_700_000_050, status: 'connected', reason: null }],
    ...over,
  });
}

export function health(over: Partial<Health> = {}): Health {
  return {
    service: { version: '0.1.0', started_at: 1_700_000_000, uptime_secs: 3_700, online_since: 1_699_000_000, paused: false, restarts: 0, restart_pending: false },
    relays: [{ url: 'ws://127.0.0.1:7777', status: 'connected', connected: true }],
    proxy: '',
    proxy_check: null,
    proxy_pool: proxyPool(),
    relay_health: [],
    chain: { ok: true, tip_height: 101, last_poll_at: 1_700_000_100, poll_secs: 10, watch_errors: 0, backend_local: true, transport: 'tcp' },
    database: { ok: true, size_bytes: 2048, invoices: 10, backups: 0, last_backup_at: null },
    warnings: [],
    ...over,
  };
}

export function metrics(over: Partial<Metrics> = {}): Metrics {
  return {
    now: 1_700_000_000,
    online_since: 1_699_000_000,
    addresses_created: 12,
    invoices: 10,
    payments: 3,
    received_sat: 150_000,
    windows: [
      { label: '24h', secs: 86_400, available: true, count: 1, sat: 50_000 },
      { label: '7d', secs: 604_800, available: true, count: 2, sat: 100_000 },
      { label: '30d', secs: 2_592_000, available: false, count: 3, sat: 150_000 },
      { label: '365d', secs: 31_536_000, available: false, count: 3, sat: 150_000 },
    ],
    sweep: { enabled: false, mode: 'payjoin', threshold_sat: 1_000_000, received_sat: 150_000, due: false },
    ...over,
  };
}

export function series(over: Partial<Series> = {}): Series {
  const points = [0, 1, 2, 3, 4, 5, 6].map((i) => ({ start: 1_699_400_000 + i * 86_400, count: i === 5 ? 2 : 0, sat: i === 5 ? 100_000 : 0 }));
  return { range: 'week', bucket_secs: 86_400, online_since: 1_699_500_000, points, ...over };
}

export function settings(over: Partial<Settings> = {}): Settings {
  return {
    name: 'deadhop',
    relays: ['ws://127.0.0.1:7777'],
    public_invoicing: true,
    pow_bits: 16,
    pow_window_secs: 600,
    min_confirmations: 1,
    default_expiry_secs: 3600,
    max_expiry_secs: 86_400,
    min_amount_sat: 1000,
    max_amount_sat: 100_000_000,
    max_open_invoices: 500,
    max_invoices_per_day: 2000,
    rate_global_per_min: 30,
    rate_per_key_per_min: 6,
    rate_requests_per_min: 1200,
    rate_requests_per_key_per_min: 120,
    poll_secs: 10,
    late_grace_secs: 86_400,
    proxies: { list: [], mode: 'failover', rotate_secs: 3600, chaos_min_secs: 300, chaos_max_secs: 900, offline: 'kill_switch', transparent: false },
    sweep: { enabled: false, mode: 'payjoin', threshold_sat: 1_000_000, max_fee_rate_sat_vb: 20, min_anonymity_set: 5 },
    ...over,
  };
}

export function settingsView(over: Partial<SettingsView> = {}): SettingsView {
  return {
    settings: settings(),
    defaults: settings(),
    overridden: [],
    fixed: { network: 'regtest', electrum: '127.0.0.1:50001', electrum_transport: 'tcp', first_address: ADDRESS, announce: false, proxy_locked: false },
    operators: [{ pubkey: PUBKEY, label: '', source: 'environment', added_at: null }],
    restart_pending: false,
    warnings: [],
    ...over,
  };
}

export function audit(over: Partial<AuditLog> = {}): AuditLog {
  return {
    entries: [
      { at: 1_700_000_000, actor: PUBKEY, action: 'update_settings', detail: '{"pow_bits":18}' },
      { at: 1_699_999_000, actor: 'service', action: 'revert_connection', detail: 'no relay connected' },
    ],
    ...over,
  };
}

export function relayStats(over: Partial<RelayStats> = {}): RelayStats {
  const points = [0, 1, 2, 3].map((i) => ({ start: 1_700_000_000 + i * 3600, ok: i === 1 ? 0 : 4, fail: i === 1 ? 2 : 0 }));
  return { relay: 'ws://127.0.0.1:7777', checks: 4, check_failures: 0, sends: 10, send_failures: 2, avg_ms: 120, points, ...over };
}

export function relayHistory(over: Partial<RelayHistory> = {}): RelayHistory {
  return { window: '1d', bucket_secs: 3600, since: 1_700_000_000, until: 1_700_014_400, relays: [relayStats()], ...over };
}
