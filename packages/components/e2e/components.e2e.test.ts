// Each framework's PayButton, built from the published files, in a real
// (headless) Chrome under a strict Content-Security-Policy, paying a real
// invoice on the local regtest stack. Requires `just up` and `just server`.
// Set CHROME_PATH to use a different Chromium-based browser.
import { execFileSync } from 'node:child_process';
import { copyFileSync, existsSync, readFileSync } from 'node:fs';
import { createServer, type Server } from 'node:http';
import { dirname, extname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import puppeteer, { type Browser, type Page } from 'puppeteer-core';
import { afterAll, beforeAll, describe, expect, it } from 'vitest';

const here = dirname(fileURLToPath(import.meta.url));
const root = resolve(here, '../../..');
const chrome = process.env.CHROME_PATH ?? '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome';
const TYPES: Record<string, string> = { '.html': 'text/html', '.js': 'text/javascript', '.css': 'text/css', '.json': 'application/json' };

function bitcoinCli(...args: string[]): string {
  return execFileSync(
    'docker',
    ['compose', '-f', resolve(root, 'deploy/docker-compose.yml'), 'exec', '-T', 'bitcoind', 'bitcoin-cli', '-regtest', '-rpcuser=cvmpay', '-rpcpassword=cvmpay-dev-only', ...args],
    { encoding: 'utf8' },
  ).trim();
}
const mine = (n: number) => bitcoinCli('generatetoaddress', String(n), bitcoinCli('-rpcwallet=miner', 'getnewaddress'));

describe.skipIf(!existsSync(chrome))('components in a real browser', () => {
  let server: Server;
  let browser: Browser;
  let base: string;
  const app = resolve(here, 'app');

  beforeAll(async () => {
    const env = { PATH: process.env.PATH, HOME: process.env.HOME, NODE_ENV: 'production' };
    execFileSync('node', ['build.mjs'], { cwd: app, stdio: 'inherit', env });
    copyFileSync(resolve(root, 'dashboard/public/config.json'), resolve(app, 'dist/config.json'));
    server = createServer((req, res) => {
      const path = join(app, 'dist', decodeURIComponent(new URL(req.url!, 'http://x').pathname));
      const file = path.endsWith('/') ? join(path, 'index.html') : path;
      if (!file.startsWith(join(app, 'dist')) || !existsSync(file)) {
        res.writeHead(404).end();
        return;
      }
      res.writeHead(200, { 'content-type': TYPES[extname(file)] ?? 'application/octet-stream' }).end(readFileSync(file));
    });
    await new Promise<void>((ok) => server.listen(0, '127.0.0.1', ok));
    const address = server.address() as { port: number };
    base = `http://127.0.0.1:${address.port}`;
    browser = await puppeteer.launch({ executablePath: chrome, headless: true });
  });

  afterAll(async () => {
    await browser?.close();
    await new Promise((done) => server?.close(() => done(undefined)));
  });

  for (const fw of ['react', 'solid', 'svelte5', 'svelte4']) {
    it(`${fw}: PayButton takes a payment from click to paid`, async () => {
      const page: Page = await browser.newPage();
      const problems: string[] = [];
      page.on('pageerror', (e) => problems.push(`pageerror: ${(e as Error).message}`));
      page.on('console', (m) => {
        if (m.type() === 'error') problems.push(`console: ${m.text()}`);
      });
      await page.goto(`${base}/${fw}/`);
      const button = await page.waitForSelector('button.cvmpay-pay');
      expect(await button!.evaluate((b) => b.textContent)).toBe('Pay 5,000 sats');
      await button!.click();

      await page.waitForSelector('.cvmpay-invoice[data-status="pending"]', { timeout: 60_000 });
      const address = await page.$eval('.cvmpay-address', (el) => el.textContent!.trim());
      expect(address).toMatch(/^bcrt1q/);
      expect(await page.$eval('.cvmpay-qr', (el) => el.getAttribute('href'))).toBe(`bitcoin:${address}?amount=0.00005000`);
      expect(await page.$eval('.cvmpay-memo', (el) => el.textContent)).toContain(`browser e2e (${fw})`);
      // The stylesheet applied (and was allowed by the CSP).
      expect(await page.$eval('.cvmpay-pay, .cvmpay-invoice', (el) => getComputedStyle(el).borderRadius)).toBe('10px');

      bitcoinCli('-rpcwallet=miner', 'sendtoaddress', address, '0.00005000');
      await page.waitForSelector('body[data-detected]', { timeout: 60_000 });
      mine(1);
      await page.waitForSelector('.cvmpay-invoice[data-status="confirmed"]', { timeout: 60_000 });
      const id = await page.$eval('body', (b) => b.getAttribute('data-paid'));
      expect(id).toMatch(/^[0-9a-f]{32}$/);
      expect(await page.$('.cvmpay-qr')).toBeNull();
      expect(problems).toEqual([]);
      await page.close();
    });
  }
});
