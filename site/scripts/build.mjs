// Build the landing page (Astro) and the docs (VitePress, under /docs) into
// one static folder, then add the files agents and crawlers look for:
// a Markdown twin of every page, llms.txt, llms-full.txt, sitemap.xml and robots.txt.
import { spawnSync } from 'node:child_process';
import { copyFileSync, readFileSync, readdirSync, rmSync, writeFileSync, existsSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { LOCAL_URL, basePath, siteUrl, sitePath } from './site-url.mjs';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const dist = join(root, 'dist');
const docsSrc = join(root, 'docs');
const SITE = siteUrl();
const PUBLIC_URL = new URL(basePath(), SITE).href; // origin plus base path

if (SITE === new URL(LOCAL_URL).origin) {
  console.warn(`SITE_URL is not set; canonical links will point at ${LOCAL_URL}. Set SITE_URL for production.`);
}

function run(cmd, args) {
  const r = spawnSync(cmd, args, { cwd: root, stdio: 'inherit', env: { ...process.env, SITE_URL: PUBLIC_URL } });
  if (r.status !== 0) process.exit(r.status ?? 1);
}

// Astro empties dist/, so it goes first; VitePress then writes dist/docs/.
rmSync(dist, { recursive: true, force: true });
run('npx', ['astro', 'build']);
run('npx', ['vitepress', 'build', 'docs']);

// ---- Docs as Markdown ------------------------------------------------------

/** Split YAML-ish front matter (only flat `key: value` pairs are used here). */
function parse(source) {
  const m = source.match(/^---\n([\s\S]*?)\n---\n/);
  const meta = {};
  if (m) {
    for (const line of m[1].split('\n')) {
      const i = line.indexOf(':');
      if (i > 0) meta[line.slice(0, i).trim()] = line.slice(i + 1).trim();
    }
  }
  return { meta, body: m ? source.slice(m[0].length) : source };
}

// Sidebar order, so llms.txt reads like the docs navigation.
const ORDER = ['index', 'quickstart', 'how-it-works', 'configuration', 'deployment', 'api', 'integrate', 'integrate-react', 'integrate-solid', 'integrate-svelte5', 'integrate-svelte4', 'integrate-javascript', 'integrate-backend', 'security', 'testing', 'design', 'roadmap', 'agents'];
const stems = readdirSync(docsSrc)
  .filter((f) => f.endsWith('.md'))
  .map((f) => f.replace(/\.md$/, ''))
  .sort((a, b) => (ORDER.indexOf(a) + 1 || 99) - (ORDER.indexOf(b) + 1 || 99));

const pages = stems.map((stem) => {
  const { meta, body } = parse(readFileSync(join(docsSrc, `${stem}.md`), 'utf8'));
  // Put the description under the title, as llms.txt-style Markdown expects.
  const markdown = body.trimStart().replace(/^# (.+)\n/m, (h) => `${h}\n> ${meta.description}\n`).trimEnd() + '\n';
  const html = stem === 'index' ? '/docs/' : `/docs/${stem}.html`;
  writeFileSync(join(dist, 'docs', `${stem}.md`), markdown);
  return { stem, title: meta.title, description: meta.description, html, md: `/docs/${stem}.md`, markdown };
});

// ---- llms.txt and llms-full.txt ------------------------------------------------

const landing = readFileSync(join(dist, 'index.md'), 'utf8');
// Site paths ("/docs/x.md") -> absolute URLs under the base path.
const abs = (p) => new URL(sitePath(p), SITE).href;
const summary = landing.match(/^> (.+)$/m)?.[1] ?? '';

const llms = [
  '# deadhop',
  '',
  `> ${summary} Self-hosted on-chain bitcoin invoicing over Nostr (ContextVM): no inbound ports, end-to-end encrypted requests, NIP-13 proof of work, a watch-only wallet and payment checks against your own Electrum server.`,
  '',
  'Every page below is Markdown. For the HTML version of a page, replace `.md` with `.html`.',
  '',
  '## Overview',
  '',
  `- [Landing page](${abs('/index.md')}): what deadhop does, its security benefits and limits`,
  '',
  '## Docs',
  '',
  ...pages.filter((p) => !['roadmap', 'agents', 'testing', 'design'].includes(p.stem)).map((p) => `- [${p.title}](${abs(p.md)}): ${p.description}`),
  '',
  '## Optional',
  '',
  ...pages.filter((p) => ['testing', 'design', 'roadmap', 'agents'].includes(p.stem)).map((p) => `- [${p.title}](${abs(p.md)}): ${p.description}`),
  `- [Everything in one file](${abs('/llms-full.txt')}): landing page and all docs pages`,
  '',
].join('\n');
writeFileSync(join(dist, 'llms.txt'), llms);

const full = [landing.trimEnd(), ...pages.map((p) => `---\n\nSource: ${abs(p.html)}\n\n${p.markdown.trimEnd()}`)].join('\n\n') + '\n';
writeFileSync(join(dist, 'llms-full.txt'), full);

// ---- sitemap.xml (one file for both halves) -------------------------------------

const vpSitemap = join(dist, 'docs', 'sitemap.xml');
if (existsSync(vpSitemap)) rmSync(vpSitemap);
const today = new Date().toISOString().slice(0, 10);
const entries = [{ loc: abs('/'), priority: '1.0' }, ...pages.map((p) => ({ loc: abs(p.html), priority: p.stem === 'index' ? '0.9' : '0.7' }))];
writeFileSync(
  join(dist, 'sitemap.xml'),
  `<?xml version="1.0" encoding="UTF-8"?>\n<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">\n${entries
    .map((e) => `  <url><loc>${e.loc}</loc><lastmod>${today}</lastmod><priority>${e.priority}</priority></url>`)
    .join('\n')}\n</urlset>\n`,
);

// ---- robots.txt -----------------------------------------------------------------

const AI_AGENTS = [
  'GPTBot',
  'OAI-SearchBot',
  'ChatGPT-User',
  'ClaudeBot',
  'Claude-User',
  'Claude-SearchBot',
  'anthropic-ai',
  'PerplexityBot',
  'Perplexity-User',
  'Google-Extended',
  'Applebot-Extended',
  'CCBot',
  'meta-externalagent',
  'Amazonbot',
  'DuckAssistBot',
  'MistralAI-User',
];
writeFileSync(
  join(dist, 'robots.txt'),
  [
    '# deadhop: public documentation. Search engines and AI agents are welcome.',
    '# Machine-readable index: /llms.txt (full text: /llms-full.txt). Every page has a .md twin.',
    '',
    'User-agent: *',
    'Allow: /',
    '',
    ...AI_AGENTS.flatMap((a) => [`User-agent: ${a}`, 'Allow: /', '']),
    `Sitemap: ${abs('/sitemap.xml')}`,
    '',
  ].join('\n'),
);

// Static hosts such as GitHub Pages serve /404.html for unknown paths.
copyFileSync(join(dist, 'docs', '404.html'), join(dist, '404.html'));

console.log(`built ${pages.length} docs pages + landing page into dist/ for ${PUBLIC_URL}`);
