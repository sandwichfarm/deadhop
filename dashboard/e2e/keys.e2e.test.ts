// The two keys in a real browser against a private service: sign in through the
// administration key, give a relay and a proxy to one key only, and what the
// dashboard says when a key would be left with nothing. Requires `just up`
// (relay and Electrum) and a service binary (DEADHOP_BIN, or a build under
// server/target). Set PROOF_DIR to also write screenshots there.
import { execFileSync } from 'node:child_process';
import { existsSync, mkdirSync, mkdtempSync, readFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import puppeteer, { type Browser, type ElementHandle, type Page } from 'puppeteer-core';
import { afterAll, beforeAll, describe, expect, it } from 'vitest';
import { dashboardDir, serverBinary, startPoolStack, type PoolStack } from './pool-stack';

const chrome = process.env.CHROME_PATH ?? '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome';
const binary = serverBinary();
const proofs = process.env.PROOF_DIR;

// The same relay under two addresses: the service sees two relays.
const RELAY_A = 'ws://127.0.0.1:7777';
const RELAY_B = 'ws://localhost:7777';

const hasText = (page: Page, text: string, timeout = 90_000) =>
  page.waitForFunction((t) => document.body.innerText.toLowerCase().includes(t), { timeout }, text.toLowerCase());

async function click(page: Page, selector: string) {
  const handle = await page.waitForSelector(selector);
  await handle!.click();
}
const button = (page: Page, name: string) => click(page, `::-p-xpath(//button[normalize-space()="${name}"])`);
/** A "used by" switch: the label that holds the checkbox for `what` and `key`. */
const chip = (page: Page, what: string, key: string) => click(page, `label:has(input[aria-label="${what} used by ${key}"])`);
/** Open `tab` afresh (the views read the service when they open) until `text` shows. */
async function reopenUntil(page: Page, tab: string, text: string, ms = 150_000) {
  const deadline = Date.now() + ms;
  for (;;) {
    await button(page, tab === 'Overview' ? 'Settings' : 'Overview');
    await button(page, tab);
    await new Promise((r) => setTimeout(r, 1500));
    if ((await page.evaluate(() => document.body.innerText)).toLowerCase().includes(text.toLowerCase())) return;
    if (Date.now() > deadline) throw new Error(`"${text}" did not show on ${tab}`);
  }
}

describe.skipIf(!existsSync(chrome) || binary === null)('two keys in a real browser', () => {
  let stack: PoolStack;
  let browser: Browser;
  let page: Page;
  const problems: string[] = [];

  async function shot(name: string, target?: ElementHandle | null) {
    if (!proofs) return;
    mkdirSync(proofs, { recursive: true });
    await new Promise((r) => setTimeout(r, 300));
    const path = join(proofs, `${name}.png`) as `${string}.png`;
    if (target) await target.screenshot({ path });
    else await page.screenshot({ path, clip: { x: 0, y: 0, width: 1280, height: 1000 } });
  }
  const card = () => page.$('section.card[aria-labelledby="s-conn"]');
  const proxies = () => page.$('section.proxy-pool');

  beforeAll(async () => {
    stack = await startPoolStack(binary!, { DEADHOP_RELAYS: `${RELAY_A},${RELAY_B}` });
    browser = await puppeteer.launch({ executablePath: chrome, headless: true, defaultViewport: { width: 1280, height: 2000, deviceScaleFactor: 2 } });
    page = await browser.newPage();
    page.on('pageerror', (e) => problems.push(`pageerror: ${(e as Error).message}`));
    page.on('console', (m) => {
      if (m.type() === 'error') problems.push(`console: ${m.text()}`);
    });
    page.on('dialog', (d) => void d.accept());
  }, 180_000);

  afterAll(async () => {
    await browser?.close();
    await stack?.stop();
  });

  it('refuses a key that is not an operator, and signs an operator in through the administration key', async () => {
    const dir = mkdtempSync(join(tmpdir(), 'deadhop-keys-'));
    execFileSync('node', ['scripts/keygen.mjs', join(dir, 'stranger')], { cwd: dashboardDir, stdio: 'ignore' });
    await page.goto(stack.url);
    await page.waitForSelector('#secret');
    await shot('01-sign-in');
    await page.type('#secret', readFileSync(join(dir, 'stranger.nsec'), 'utf8').trim());
    await button(page, 'Sign in');
    await hasText(page, 'This key is not an operator of this service.');
    await shot('02-not-an-operator');

    await page.goto(stack.url);
    await page.type('#secret', stack.nsec);
    await button(page, 'Sign in');
    await hasText(page, 'Addresses created');
    await hasText(page, 'Public invoices');
    await hasText(page, 'Administration');
    await shot('03-overview-two-keys', await page.$('.key-strips'));
  }, 180_000);

  it('gives a relay to one key only, and shows it', async () => {
    await button(page, 'Settings');
    await hasText(page, 'Keys');
    await shot('04-settings-keys-and-relays', await card());
    // The second relay is for the public key only.
    await chip(page, RELAY_B, 'Administration');
    await shot('05-relay-for-one-key', await card());
    await button(page, 'Save changes');
    await hasText(page, 'No unsaved changes.');
    for (let i = 0; i < 40 && !(await page.$('.key-badges')); i++) {
      await button(page, 'Settings');
      await button(page, 'Overview');
      await new Promise((r) => setTimeout(r, 1500));
    }
    await page.waitForSelector('.key-badges', { timeout: 5_000 });
    await shot('06-overview-relay-badges');
    const keys = await page.$eval('.key-strips', (e) => (e as HTMLElement).innerText);
    expect(keys).toContain('Do not publish it');
    expect(keys).not.toContain(RELAY_B);
  }, 240_000);

  it('stops a save that would leave a key without a relay, and warns before the page loses its own', async () => {
    await button(page, 'Settings');
    await hasText(page, 'Keys');
    // The second relay is already for the public key only: take the first away from
    // the administration key too, and it has no relay at all.
    await chip(page, RELAY_A, 'Administration');
    await button(page, 'Save changes');
    await hasText(page, 'No relay is assigned to Administration');
    await shot('07-blocked-no-relay');
    await button(page, 'Discard');
    await hasText(page, 'No unsaved changes.');

    // The page signs in over the first relay: giving the administration key only
    // the second one is allowed, but the page would lose its way in.
    await chip(page, RELAY_B, 'Administration');
    await chip(page, RELAY_A, 'Administration');
    await hasText(page, 'Keep one of them for that key');
    await shot('08-lockout-warning', await card());
    await button(page, 'Discard');
    await hasText(page, 'No unsaved changes.');
  }, 240_000);

  it('gives each key its own proxy, and stops a save that leaves one without', async () => {
    await button(page, 'Settings');
    await hasText(page, 'Proxy list, in order');
    // First proxy for the public key, second for the administration key.
    await chip(page, 'Proxy 1', 'Administration');
    await chip(page, 'Proxy 2', 'Public invoices');
    await shot('09-proxies-per-key', await proxies());
    await button(page, 'Save changes');
    await hasText(page, 'No unsaved changes.');
    await reopenUntil(page, 'Settings', 'Administration: using');
    await shot('10-proxy-in-use-per-key', await proxies());

    // Both proxies for the public key leaves the administration key with none.
    await chip(page, 'Proxy 2', 'Public invoices');
    await chip(page, 'Proxy 2', 'Administration');
    await button(page, 'Save changes');
    await hasText(page, 'No proxy is assigned to Administration');
    await shot('11-blocked-no-proxy');
    await button(page, 'Discard');
    await hasText(page, 'No unsaved changes.');
  }, 300_000);

  it('raised no errors in the page', () => {
    expect(problems).toEqual([]);
  });
});
