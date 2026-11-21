// Drives the production build of the dashboard in a real (headless) Chrome
// against the local regtest stack. Requires `just up` and `just server`.
// Set CHROME_PATH to use a different Chromium-based browser.
import { execFileSync } from 'node:child_process';
import { existsSync, readFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import puppeteer, { type Browser, type Page } from 'puppeteer-core';
import { preview, type PreviewServer } from 'vite';
import { afterAll, beforeAll, describe, expect, it } from 'vitest';

const here = dirname(fileURLToPath(import.meta.url));
const root = resolve(here, '../..');
const chrome = process.env.CHROME_PATH ?? '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome';

function bitcoinCli(...args: string[]): string {
  return execFileSync(
    'docker',
    ['compose', '-f', resolve(root, 'deploy/docker-compose.yml'), 'exec', '-T', 'bitcoind', 'bitcoin-cli', '-regtest', '-rpcuser=deadhop', '-rpcpassword=deadhop-dev-only', ...args],
    { encoding: 'utf8' },
  ).trim();
}

// Case-insensitive: labels and stamps may be uppercased by CSS, which innerText reflects.
const hasText = (page: Page, text: string, timeout = 60_000) =>
  page.waitForFunction((t) => document.body.innerText.toLowerCase().includes(t), { timeout }, text.toLowerCase());

async function clickButton(page: Page, name: string) {
  const handle = await page.waitForSelector(`::-p-xpath(//button[normalize-space()="${name}"])`);
  await handle!.click();
}

describe.skipIf(!existsSync(chrome))('dashboard in a real browser', () => {
  let server: PreviewServer;
  let browser: Browser;
  let url: string;
  const problems: string[] = [];

  async function open(): Promise<Page> {
    const page = await browser.newPage();
    page.on('pageerror', (e) => problems.push(`pageerror: ${(e as Error).message}`));
    page.on('console', (m) => {
      if (m.type() === 'error') problems.push(`console: ${m.text()}`);
    });
    await page.goto(url);
    return page;
  }

  beforeAll(async () => {
    const dashboard = resolve(here, '..');
    // A real production build, in its own process: building from inside the
    // test runner would resolve Svelte's server entry instead of the browser one.
    // It also must not inherit the test runner's environment (VITEST, NODE_ENV=test).
    const env = { PATH: process.env.PATH, HOME: process.env.HOME, NODE_ENV: 'production' };
    execFileSync('npx', ['vite', 'build', '--logLevel', 'error'], { cwd: dashboard, stdio: 'inherit', env });
    server = await preview({ root: dashboard, logLevel: 'error', preview: { port: 0, host: '127.0.0.1' } });
    url = server.resolvedUrls!.local[0];
    browser = await puppeteer.launch({ executablePath: chrome, headless: true });
  });

  afterAll(async () => {
    await browser?.close();
    await new Promise((done) => server?.httpServer.close(() => done(undefined)));
  });

  it('takes a guest from sign-in to a confirmed payment, and shows it to the operator', async () => {
    // --- customer ---
    const page = await open();
    await hasText(page, 'Use browser extension');
    await clickButton(page, 'Continue as guest');
    await hasText(page, 'New invoice');
    await page.type('#amount', '60000');
    await page.type('#memo', 'browser order');
    await clickButton(page, 'Create invoice');

    await hasText(page, 'Awaiting payment');
    expect(new URL(page.url()).hash).toMatch(/^#\/pay\/[0-9a-f]{32}$/);
    await hasText(page, '0.00060000 BTC');
    const address = await page.$eval('.address', (el) => el.textContent!.trim());
    expect(address).toMatch(/^bcrt1q/);
    const href = await page.$eval('a.qr-link', (el) => el.getAttribute('href'));
    expect(href).toBe(`bitcoin:${address}?amount=0.00060000`);
    expect(await page.$('svg.qr path')).not.toBeNull();

    bitcoinCli('-rpcwallet=miner', 'sendtoaddress', address, '0.00060000');
    await hasText(page, 'Payment seen, confirming');
    bitcoinCli('generatetoaddress', '1', bitcoinCli('-rpcwallet=miner', 'getnewaddress'));
    await hasText(page, 'Paid');
    expect(await page.$('svg.qr')).toBeNull();

    // A payment link opens directly, with no sign-in.
    const link = await open();
    await link.goto(page.url());
    await hasText(link, 'Paid');
    await hasText(link, 'browser order');

    // --- operator ---
    const admin = await open();
    await hasText(admin, 'Use browser extension');
    await admin.type('#secret', readFileSync(resolve(root, 'data/dev/operator.nsec'), 'utf8').trim());
    await clickButton(admin, 'Sign in');
    // Overview: health, totals, the payment chart.
    await hasText(admin, 'Addresses created');
    await hasText(admin, '1 of 1 connected');
    await hasText(admin, 'Healthy');
    expect(await admin.$('svg[role="img"] path.line')).not.toBeNull();
    await clickButton(admin, '1 year');
    await hasText(admin, 'Show as table');
    // Relay connectivity: a page of its own, reached from Overview.
    await clickButton(admin, 'Connectivity');
    await hasText(admin, 'health check');
    await clickButton(admin, '← Overview');
    // The other views.
    await clickButton(admin, 'Settings');
    await hasText(admin, 'Proxy list, in order');
    await clickButton(admin, 'Operators');
    await hasText(admin, 'from DEADHOP_ADMINS');
    await hasText(admin, 'Audit log');
    await clickButton(admin, 'Maintenance');
    await hasText(admin, 'Wipe invoices');
    // A backup shows up in the list of backups on the host.
    await clickButton(admin, 'Back up now');
    await hasText(admin, 'Backup written on the service host');
    const newest = await admin.waitForSelector('ul.backup-list li .mono');
    expect(await newest!.evaluate((el) => el.textContent)).toMatch(/^deadhop-\d{12}-\d{3}\.sqlite$/);
    await clickButton(admin, 'Invoices');
    await hasText(admin, 'browser order');
    await hasText(admin, 'Open payment page');
    const row = await admin.$eval('tbody tr', (el) => (el as HTMLElement).innerText);
    expect(row).toContain('60,000 sats');
    expect(row).toContain('Paid');

    await clickButton(admin, 'Sign out');
    await hasText(admin, 'Use browser extension');

    // No script errors and no Content-Security-Policy violations anywhere.
    expect(problems).toEqual([]);
  });
});
