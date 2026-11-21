// A private deadhop service for the proxy-pool scenario: its own data directory
// and keys, two local SOCKS5 proxies the test can stop, the dev relay and
// Electrum server (`just up`), and a fresh production build of the dashboard.
import { execFileSync, spawn, type ChildProcess } from 'node:child_process';
import { existsSync, mkdtempSync, readFileSync, writeFileSync } from 'node:fs';
import net, { type Server } from 'node:net';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { preview, type PreviewServer } from 'vite';

const here = dirname(fileURLToPath(import.meta.url));
export const dashboardDir = resolve(here, '..');
const root = resolve(dashboardDir, '..');

/** A SOCKS5 proxy with no login: enough for the service's relay connections. */
export class Socks5 {
  private server: Server | null = null;
  private sockets = new Set<net.Socket>();
  port = 0;

  async start(port = 0): Promise<void> {
    this.server = net.createServer((c) => this.serve(c));
    await new Promise<void>((ok) => this.server!.listen(port, '127.0.0.1', ok));
    this.port = (this.server.address() as net.AddressInfo).port;
  }

  /** Stop accepting, and cut what is open. */
  async stop(): Promise<void> {
    const s = this.server;
    this.server = null;
    this.sockets.forEach((x) => x.destroy());
    if (s) await new Promise((ok) => s.close(ok));
  }

  get url(): string {
    return `socks5://127.0.0.1:${this.port}`;
  }

  private serve(c: net.Socket) {
    this.sockets.add(c);
    c.on('close', () => this.sockets.delete(c));
    c.on('error', () => c.destroy());
    c.once('data', (greeting: Buffer) => {
      if (greeting[0] !== 5) return c.destroy();
      c.write(Buffer.from([5, 0]));
      c.once('data', (req: Buffer) => {
        const atyp = req[3];
        let host: string;
        let end: number;
        if (atyp === 1) {
          host = [...req.subarray(4, 8)].join('.');
          end = 8;
        } else if (atyp === 3) {
          host = req.subarray(5, 5 + req[4]).toString();
          end = 5 + req[4];
        } else return c.destroy();
        const port = req.readUInt16BE(end);
        const up = net.connect(port, host);
        this.sockets.add(up);
        up.on('close', () => this.sockets.delete(up));
        up.on('error', () => {
          c.write(Buffer.from([5, 5, 0, 1, 0, 0, 0, 0, 0, 0]));
          c.destroy();
        });
        up.on('connect', () => {
          c.write(Buffer.from([5, 0, 0, 1, 0, 0, 0, 0, 0, 0]));
          c.pipe(up);
          up.pipe(c);
        });
        c.on('close', () => up.destroy());
      });
    });
  }
}

export interface PoolStack {
  url: string;
  nsec: string;
  proxies: Socks5[];
  stop(): Promise<void>;
}

/** Where the service binary is: DEADHOP_BIN, else a build under server/target. */
export function serverBinary(): string | null {
  const candidates = [process.env.DEADHOP_BIN, resolve(root, 'server/target/release/deadhop'), resolve(root, 'server/target/debug/deadhop')];
  return candidates.find((p): p is string => !!p && existsSync(p)) ?? null;
}

export async function startPoolStack(binary: string, env: Record<string, string> = {}): Promise<PoolStack> {
  const dir = mkdtempSync(join(tmpdir(), 'deadhop-pool-'));
  execFileSync('node', ['scripts/keygen.mjs', join(dir, 'operator')], { cwd: dashboardDir, stdio: 'ignore' });
  const nsec = readFileSync(join(dir, 'operator.nsec'), 'utf8').trim();
  const proxies = [new Socks5(), new Socks5()];
  await Promise.all(proxies.map((p) => p.start()));

  const child: ChildProcess = spawn(binary, [], {
    stdio: ['ignore', 'ignore', 'inherit'],
    env: {
      PATH: process.env.PATH,
      DEADHOP_NAME: 'deadhop (proxy pool)',
      DEADHOP_RELAYS: 'ws://127.0.0.1:7777',
      DEADHOP_ELECTRUM: '127.0.0.1:50001',
      DEADHOP_NETWORK: 'regtest',
      DEADHOP_XPUB: readFileSync(resolve(root, 'data/dev/xpub'), 'utf8').trim(),
      DEADHOP_ADMINS: readFileSync(join(dir, 'operator.pubkey'), 'utf8').trim(),
      DEADHOP_PUBLIC_INVOICING: 'true',
      DEADHOP_POW_BITS: '8',
      DEADHOP_POLL_SECS: '2',
      DEADHOP_DATA_DIR: join(dir, 'server'),
      DEADHOP_PROXY: proxies[0].url,
      DEADHOP_PROXIES: proxies[1].url,
      ...env,
    },
  });
  const pubkeyFile = join(dir, 'server', 'server.pubkey');
  for (let i = 0; i < 150 && !existsSync(pubkeyFile); i++) await new Promise((r) => setTimeout(r, 200));
  const serverPubkey = readFileSync(pubkeyFile, 'utf8').trim();

  const out = join(dir, 'dist');
  const buildEnv = { PATH: process.env.PATH, HOME: process.env.HOME, NODE_ENV: 'production' };
  execFileSync('npx', ['vite', 'build', '--logLevel', 'error', '--outDir', out, '--emptyOutDir'], { cwd: dashboardDir, stdio: 'inherit', env: buildEnv });
  writeFileSync(join(out, 'config.json'), JSON.stringify({ serverPubkey, relays: ['ws://127.0.0.1:7777'] }));
  const web: PreviewServer = await preview({ root: dashboardDir, build: { outDir: out }, logLevel: 'error', preview: { port: 0, host: '127.0.0.1' } });

  return {
    url: web.resolvedUrls!.local[0],
    nsec,
    proxies,
    async stop() {
      child.kill('SIGTERM');
      await Promise.all(proxies.map((p) => p.stop()));
      await new Promise((ok) => web.httpServer.close(() => ok(undefined)));
    },
  };
}
