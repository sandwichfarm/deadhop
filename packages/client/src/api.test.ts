import { beforeEach, describe, expect, it, vi } from 'vitest';
import { PrivateKeySigner } from '@contextvm/sdk';
import { ApiError, PayApi, connect, toApiError, withTimeout } from './api.ts';
import { CTXVM_KIND, PowError } from './pow.ts';
import { InvoiceSchema } from './schemas.ts';
import { PUBKEY, info, invoice, toolResult } from './test-helpers.ts';

const mocks = vi.hoisted(() => ({
  transports: [] as Array<{ options: Record<string, unknown> }>,
  clients: [] as Array<{ info: Record<string, unknown> }>,
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
    constructor(readonly info: Record<string, unknown>) {
      mocks.clients.push(this);
    }
    connect = mocks.connect;
    callTool = mocks.callTool;
    close = mocks.close;
  },
}));

const config = { serverPubkey: PUBKEY, relays: ['ws://127.0.0.1:7777'] };

describe('PayApi', () => {
  it('calls each tool with its arguments and validates the result', async () => {
    const caller = vi.fn(async (name: string) => {
      const results: Record<string, unknown> = {
        get_info: info(),
        create_invoice: invoice(),
        get_invoice: invoice({ status: 'confirmed' }),
      };
      return toolResult(results[name]);
    });
    const api = new PayApi(caller);
    expect(await api.getInfo()).toEqual(info());
    expect(await api.createInvoice({ amount_sat: 50_000, memo: 'table 4' })).toEqual(invoice());
    expect((await api.getInvoice(invoice().id)).status).toBe('confirmed');
    expect(await api.call('get_invoice', { id: invoice().id }, InvoiceSchema)).toMatchObject({ status: 'confirmed' });
    expect(caller.mock.calls.map((c) => c)).toEqual([
      ['get_info', {}],
      ['create_invoice', { amount_sat: 50_000, memo: 'table 4' }],
      ['get_invoice', { id: invoice().id }],
      ['get_invoice', { id: invoice().id }],
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
    const thrown = (e: unknown) => new PayApi(async () => Promise.reject(e)).getInfo();
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
    mocks.clients.length = 0;
    mocks.connect.mockReset().mockResolvedValue(undefined);
    mocks.close.mockReset().mockResolvedValue(undefined);
    mocks.callTool.mockReset();
  });

  it('opens an encrypted session and mines work for the public', async () => {
    mocks.callTool.mockResolvedValue(toolResult(info({ pow_bits: 6 })));
    const signer = new PrivateKeySigner();
    const session = await connect(signer, config, { mining: { batch: 64 }, clientName: 'shop' });
    expect(session.pubkey).toBe(await signer.getPublicKey());
    expect(session.info.pow_bits).toBe(6);
    expect(mocks.callTool).toHaveBeenCalledWith({ name: 'get_info', arguments: {} });
    expect(mocks.clients[0].info).toMatchObject({ name: 'shop' });

    const options = mocks.transports[0].options as { signer: PrivateKeySigner; relayHandler: string[]; serverPubkey: string; encryptionMode: string };
    expect(options.relayHandler).toEqual(config.relays);
    expect(options.serverPubkey).toBe(PUBKEY);
    expect(options.encryptionMode).toBe('required');
    const signed = await options.signer.signEvent(create as never);
    expect(signed.tags.some((t) => t[0] === 'nonce' && t[2] === '6')).toBe(true);

    // The raw caller reaches the same connection.
    await session.callTool('get_info', {});
    expect(mocks.callTool).toHaveBeenLastCalledWith({ name: 'get_info', arguments: {} });
    await session.close();
    expect(mocks.close).toHaveBeenCalledTimes(1);
  });

  it('does not mine for operators', async () => {
    mocks.callTool.mockResolvedValue(toolResult(info({ is_admin: true, pow_bits: 20 })));
    await connect(new PrivateKeySigner(), config);
    expect(mocks.clients[0].info).toMatchObject({ name: 'cvmpay-client' });
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
    mocks.connect.mockReturnValue(new Promise(() => undefined));
    await expect(connect(new PrivateKeySigner(), config, { timeoutMs: 10 })).rejects.toMatchObject({
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
