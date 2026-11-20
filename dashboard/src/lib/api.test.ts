import { beforeEach, describe, expect, it, vi } from 'vitest';
import { PrivateKeySigner } from '@contextvm/sdk';
import { ApiError, PayApi, connect, toApiError, withTimeout } from './api';
import { CTXVM_KIND, PowError } from '@deadhop/client';
import { PUBKEY, adminInvoice, audit, health, info, invoice, metrics, series, settingsView, stats, toolResult } from '../test-helpers';

const mocks = vi.hoisted(() => ({
  transports: [] as Array<{ options: Record<string, unknown> }>,
  connect: vi.fn(),
  callTool: vi.fn(),
  close: vi.fn(),
}));

vi.mock('@contextvm/sdk', async (original) => {
  const actual = await original<typeof import('@contextvm/sdk')>();
  class NostrClientTransport {
    constructor(readonly options: Record<string, unknown>) {
      mocks.transports.push(this);
    }
  }
  return { ...actual, NostrClientTransport };
});

vi.mock('@contextvm/mcp-sdk/client', () => ({
  Client: class {
    connect = mocks.connect;
    callTool = mocks.callTool;
    close = mocks.close;
  },
}));

const config = { serverPubkey: PUBKEY, relays: ['ws://127.0.0.1:7777'] };

describe('PayApi', () => {
  it('calls every management tool and validates the result', async () => {
    const backup = { file: 'deadhop-001700000000-000.sqlite', size_bytes: 1, created_at: 1 };
    const results: Record<string, unknown> = {
      get_health: health(),
      get_metrics: metrics(),
      get_payment_series: series(),
      get_settings: settingsView(),
      update_settings: settingsView(),
      reset_settings: settingsView(),
      add_operator: settingsView(),
      remove_operator: settingsView(),
      control_service: { paused: true, restart_pending: false },
      backup_database: backup,
      wipe_database: { deleted: 2, backup },
      get_audit_log: audit(),
    };
    const caller = vi.fn(async (name: string) => toolResult(results[name]));
    const api = new PayApi(caller);
    expect(await api.getHealth()).toEqual(health());
    expect(await api.getMetrics()).toEqual(metrics());
    expect(await api.getSeries('week')).toEqual(series());
    expect(await api.getSettings()).toEqual(settingsView());
    await api.updateSettings({ pow_bits: 20, sweep: { enabled: true } });
    await api.resetSettings();
    await api.addOperator(PUBKEY, 'till');
    await api.removeOperator(PUBKEY);
    expect(await api.control('stop')).toEqual({ paused: true, restart_pending: false });
    expect(await api.backup()).toEqual(backup);
    expect((await api.wipe({ confirm: 'WIPE', force: true })).deleted).toBe(2);
    expect((await api.getAuditLog()).entries).toHaveLength(2);
    await api.getAuditLog(5);
    expect(caller.mock.calls).toEqual([
      ['get_health', {}],
      ['get_metrics', {}],
      ['get_payment_series', { range: 'week' }],
      ['get_settings', {}],
      ['update_settings', { patch: { pow_bits: 20, sweep: { enabled: true } } }],
      ['reset_settings', {}],
      ['add_operator', { pubkey: PUBKEY, label: 'till' }],
      ['remove_operator', { pubkey: PUBKEY }],
      ['control_service', { action: 'stop' }],
      ['backup_database', {}],
      ['wipe_database', { confirm: 'WIPE', force: true }],
      ['get_audit_log', { limit: 50 }],
      ['get_audit_log', { limit: 5 }],
    ]);
  });

  it('rejects a series for another range and unexpected management fields', async () => {
    const api = new PayApi(async () => toolResult(series()));
    await expect(api.getSeries('day')).rejects.toMatchObject({ code: 'bad_response' });
    const leaky = new PayApi(async () => toolResult({ ...health(), secret: 'x' }));
    await expect(leaky.getHealth()).rejects.toMatchObject({ code: 'bad_response' });
    const badFile = new PayApi(async () => toolResult({ file: '../../etc/passwd', size_bytes: 1, created_at: 1 }));
    await expect(badFile.backup()).rejects.toMatchObject({ code: 'bad_response' });
    const paused = new PayApi(async () => {
      throw { data: { code: 'paused' } };
    });
    await expect(paused.createInvoice({ amount_sat: 1 })).rejects.toMatchObject({ code: 'paused', message: 'Invoicing is stopped by the operator.' });
  });

  it('calls each tool with its arguments and validates the result', async () => {
    const caller = vi.fn(async (name: string) => {
      const results: Record<string, unknown> = {
        get_info: info(),
        create_invoice: invoice(),
        get_invoice: invoice({ status: 'confirmed' }),
        list_invoices: { invoices: [adminInvoice()], next_before: null },
        get_stats: stats(),
      };
      return toolResult(results[name]);
    });
    const api = new PayApi(caller);
    expect(await api.getInfo()).toEqual(info());
    expect(await api.createInvoice({ amount_sat: 50_000, memo: 'table 4' })).toEqual(invoice());
    expect((await api.getInvoice(invoice().id)).status).toBe('confirmed');
    expect((await api.listInvoices({ status: 'pending', limit: 10 })).invoices).toHaveLength(1);
    expect((await api.listInvoices()).next_before).toBeNull();
    expect(await api.getStats()).toEqual(stats());
    expect(caller.mock.calls.map((c) => c)).toEqual([
      ['get_info', {}],
      ['create_invoice', { amount_sat: 50_000, memo: 'table 4' }],
      ['get_invoice', { id: invoice().id }],
      ['list_invoices', { status: 'pending', limit: 10 }],
      ['list_invoices', {}],
      ['get_stats', {}],
    ]);
  });

  it('rejects responses that do not match the schema', async () => {
    for (const raw of [toolResult({ ...invoice(), evil: true }), toolResult(undefined), {}, null, 'text']) {
      const api = new PayApi(async () => raw);
      await expect(api.getInvoice(invoice().id)).rejects.toMatchObject({ code: 'bad_response' });
    }
  });

  it('rejects an answer about a different invoice or amount', async () => {
    const api = new PayApi(async () => toolResult(invoice()));
    await expect(api.getInvoice('f'.repeat(32))).rejects.toMatchObject({ code: 'bad_response' });
    await expect(api.createInvoice({ amount_sat: 49_999 })).rejects.toMatchObject({ code: 'bad_response' });
  });

  it('reports tool-level errors as invalid parameters', async () => {
    const api = new PayApi(async () => ({ isError: true, content: [{ type: 'text', text: 'unknown field' }] }));
    await expect(api.createInvoice({ amount_sat: 1 })).rejects.toMatchObject({ code: 'invalid_params' });
  });

  it('maps thrown errors to stable codes', async () => {
    const thrown = (e: unknown) => new PayApi(async () => Promise.reject(e)).getStats();
    await expect(thrown({ code: -32001, data: { code: 'unauthorized' } })).rejects.toMatchObject({ code: 'unauthorized' });
    await expect(thrown({ data: { code: 'made_up' } })).rejects.toMatchObject({ code: 'network' });
    await expect(thrown({ data: { code: 7 } })).rejects.toMatchObject({ code: 'network' });
    await expect(thrown(new Error('socket closed'))).rejects.toMatchObject({ code: 'network' });
    await expect(thrown(null)).rejects.toMatchObject({ code: 'network' });
    await expect(thrown(new PowError('too slow'))).rejects.toMatchObject({
      code: 'pow_failed',
      message: 'Could not compute the proof of work. (too slow)',
    });
  });
});

describe('ApiError', () => {
  it('carries a readable message and passes through unchanged', () => {
    const e = new ApiError('rate_limited');
    expect(e.message).toContain('Too many requests');
    expect(e.name).toBe('ApiError');
    expect(toApiError(e)).toBe(e);
    expect(new ApiError('network', 'relay down').message).toBe('Could not reach the service. (relay down)');
  });
});

describe('connect', () => {
  const create = { kind: CTXVM_KIND, tags: [], created_at: 1_700_000_000, content: JSON.stringify({ method: 'tools/call', params: { name: 'create_invoice' } }) };

  beforeEach(() => {
    mocks.transports.length = 0;
    mocks.connect.mockReset().mockResolvedValue(undefined);
    mocks.close.mockReset().mockResolvedValue(undefined);
    mocks.callTool.mockReset();
  });

  it('opens an encrypted session and mines work for the public', async () => {
    mocks.callTool.mockResolvedValue(toolResult(info({ pow_bits: 6 })));
    const signer = new PrivateKeySigner();
    const session = await connect(signer, config, { batch: 64 });
    expect(session.pubkey).toBe(await signer.getPublicKey());
    expect(session.info.pow_bits).toBe(6);
    expect(mocks.callTool).toHaveBeenCalledWith({ name: 'get_info', arguments: {} });

    const options = mocks.transports[0].options as { signer: PrivateKeySigner; relayHandler: string[]; serverPubkey: string; encryptionMode: string };
    expect(options.relayHandler).toEqual(config.relays);
    expect(options.serverPubkey).toBe(PUBKEY);
    expect(options.encryptionMode).toBe('required');
    const signed = await options.signer.signEvent(create as never);
    expect(signed.tags.some((t) => t[0] === 'nonce' && t[2] === '6')).toBe(true);

    await session.close();
    expect(mocks.close).toHaveBeenCalledTimes(1);
  });

  it('does not mine for operators', async () => {
    mocks.callTool.mockResolvedValue(toolResult(info({ is_admin: true, pow_bits: 20 })));
    await connect(new PrivateKeySigner(), config);
    const options = mocks.transports[0].options as { signer: PrivateKeySigner };
    const signed = await options.signer.signEvent(create as never);
    expect(signed.tags.some((t) => t[0] === 'nonce')).toBe(false);
  });

  it('closes the client and reports a typed error when connecting fails', async () => {
    mocks.connect.mockRejectedValue(new Error('no relay'));
    await expect(connect(new PrivateKeySigner(), config)).rejects.toMatchObject({ code: 'network' });
    expect(mocks.close).toHaveBeenCalledTimes(1);

    // A failing close must not mask the original problem.
    mocks.connect.mockResolvedValue(undefined);
    mocks.callTool.mockResolvedValue(toolResult({ nonsense: true }));
    mocks.close.mockRejectedValue(new Error('already closed'));
    await expect(connect(new PrivateKeySigner(), config)).rejects.toMatchObject({ code: 'bad_response' });
  });

  it('gives up with a timeout when the service never answers', async () => {
    mocks.close.mockReset().mockResolvedValue(undefined);
    mocks.connect.mockReturnValue(new Promise(() => undefined));
    await expect(connect(new PrivateKeySigner(), config, {}, 10)).rejects.toMatchObject({
      code: 'timeout',
      message: expect.stringContaining('did not answer'),
    });
    expect(mocks.close).toHaveBeenCalledTimes(1);
  });
});

describe('withTimeout', () => {
  it('passes the result through when the work finishes first', async () => {
    expect(await withTimeout(Promise.resolve(7), 1000)).toBe(7);
  });
});
