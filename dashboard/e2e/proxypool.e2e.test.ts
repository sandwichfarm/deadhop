// The proxy pool in a real browser against a private service: two proxies from
// the environment, a reorder, a mode change, and what the dashboard shows when
// one and then every proxy stops answering. Requires `just up` (relay and
// Electrum) and a service binary (DEADHOP_BIN, or a build under server/target).
// Set PROOF_DIR to also write screenshots there.
import { existsSync, mkdirSync } from 'node:fs';
import { join } from 'node:path';
import puppeteer, { type Browser, type ElementHandle, type Page } from 'puppeteer-core';
import { afterAll, beforeAll, describe, expect, it } from 'vitest';
import { serverBinary, startPoolStack, type PoolStack } from './pool-stack';

const chrome = process.env.CHROME_PATH ?? '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome';
const binary = serverBinary();
const proofs = process.env.PROOF_DIR;

const hasText = (page: Page, text: string, timeout = 90_000) =>
  page.waitForFunction((t) => document.body.innerText.toLowerCase().includes(t), { timeout }, text.toLowerCase());
const gone = (page: Page, text: string, timeout = 90_000) =>
  page.waitForFunction((t) => !document.body.innerText.toLowerCase().includes(t), { timeout }, text.toLowerCase());

async function click(page: Page, selector: string) {
  const handle = await page.waitForSelector(selector);
  await handle!.click();
}
const button = (page: Page, name: string) => click(page, `::-p-xpath(//button[normalize-space()="${name}"])`);
const byLabel = (page: Page, label: string) => click(page, `[aria-label="${label}"]`);
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
/** Radio inputs are visually hidden: click the label that holds them. */
const choose = (page: Page, text: string) => click(page, `::-p-xpath(//label[contains(normalize-space(.), "${text}")])`);
const names = (page: Page) => page.$$eval('li.proxy-row', (rows) => rows.map((r) => r.querySelector('code')!.textContent));

describe.skipIf(!existsSync(chrome) || binary === null)('proxy pool in a real browser', () => {
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
    else await page.screenshot({ path, clip: { x: 0, y: 0, width: 1280, height: 560 } });
  }
  const section = () => page.$('section.proxy-pool');

  beforeAll(async () => {
    stack = await startPoolStack(binary!);
    browser = await puppeteer.launch({ executablePath: chrome, headless: true, defaultViewport: { width: 1280, height: 2000, deviceScaleFactor: 2 } });
    page = await browser.newPage();
    page.on('pageerror', (e) => problems.push(`pageerror: ${(e as Error).message}`));
    page.on('console', (m) => {
      if (m.type() === 'error') problems.push(`console: ${m.text()}`);
    });
    page.on('dialog', (d) => void d.accept());
    await page.goto(stack.url);
    await page.type('#secret', stack.nsec);
    await button(page, 'Sign in');
    await hasText(page, 'Addresses created');
  }, 180_000);

  afterAll(async () => {
    await browser?.close();
    await stack?.stop();
  });

  it('lists the environment proxies, reorders them, and saves a strategy', async () => {
    await hasText(page, 'Reachable');
    await shot('08-overview-proxy-active');
    await button(page, 'Settings');
    await hasText(page, 'Proxy list, in order');
    await hasText(page, 'in use');
    const [first, second] = stack.proxies.map((p) => p.url);
    expect(await names(page)).toEqual([first, second]);
    await shot('01-proxy-list-before', await section());

    await byLabel(page, 'Move proxy 2 up');
    expect(await names(page)).toEqual([second, first]);
    await shot('02-proxy-list-after-reorder', await section());
    await button(page, 'Save changes');
    await hasText(page, 'No unsaved changes.');
    expect(await names(page)).toEqual([second, first]);

    await button(page, 'Create proxy');
    await shot('03-create-proxy-form', await section());
    await button(page, 'Cancel');

    await choose(page, 'Round-robin');
    await shot('04-mode-round-robin', await section());
    await choose(page, 'Chaos');
    await shot('05-mode-chaos', await section());
    await button(page, 'Save changes');
    await hasText(page, 'No unsaved changes.');
    await reopenUntil(page, 'Settings', 'next switch in', 60_000);
    await shot('06-strategy-active-chaos', await section());
    await choose(page, 'Operators only');
    await shot('07-offline-policy', await section());
    await button(page, 'Save changes');
    await hasText(page, 'No unsaved changes.');
  }, 180_000);

  it('shows rotation suspended with one proxy left, then offline with none', async () => {
    // Failover order is now [second, first]; chaos needs two reachable proxies.
    await stack.proxies[0].stop();
    await reopenUntil(page, 'Settings', 'Suspended: rotation needs 2 reachable proxies, 1 is');
    await shot('09-strategy-suspended', await section());
    await reopenUntil(page, 'Overview', 'Rotation suspended');
    await shot('10-overview-proxy-suspended');

    await stack.proxies[1].stop();
    await reopenUntil(page, 'Overview', 'only operators are served');
    await hasText(page, 'No proxy reachable');
    await shot('11-overview-proxy-offline');
    await reopenUntil(page, 'Settings', 'Offline: no proxy is reachable');
    await shot('12-strategy-offline', await section());
  }, 420_000);

  it('greys the whole section out for a transparent proxy', async () => {
    await page.click('.transparent-toggle input');
    await hasText(page, 'The service trusts it and checks nothing');
    await shot('13-transparent-greyed-out', await section());
    await button(page, 'Save changes');
    await hasText(page, 'No unsaved changes.');
    await button(page, 'Overview');
    await hasText(page, 'Transparent proxy');
    await shot('14-overview-proxy-transparent');
    await gone(page, 'No proxy reachable');
    expect(problems).toEqual([]);
  });
});
