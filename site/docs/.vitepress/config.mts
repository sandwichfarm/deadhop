import { resolve } from 'node:path';
import { defineConfig, type HeadConfig } from 'vitepress';
import { siteUrl } from '../../scripts/site-url.mjs';

const SITE = siteUrl();
const BASE = '/docs/';
const DESCRIPTION =
  'Documentation for cvmpay: self-hosted on-chain bitcoin invoicing over Nostr with no inbound ports, a watch-only wallet and proof of work against abuse.';

// "foo.md" -> "/docs/foo.html" (canonical) and "/docs/foo.md" (Markdown twin).
function urls(relativePath: string) {
  const stem = relativePath.replace(/\.md$/, '');
  const html = stem === 'index' ? BASE : `${BASE}${stem}.html`;
  return { html: new URL(html, SITE).href, md: `${BASE}${stem}.md` };
}

export default defineConfig({
  base: BASE,
  outDir: resolve(import.meta.dirname, '../../dist/docs'),
  lang: 'en',
  title: 'cvmpay docs',
  titleTemplate: ':title · cvmpay docs',
  description: DESCRIPTION,
  cleanUrls: false,
  lastUpdated: false,
  appearance: true,
  sitemap: { hostname: SITE + BASE },
  head: [
    ['link', { rel: 'icon', href: `${BASE}favicon.svg`, type: 'image/svg+xml' }],
    ['meta', { name: 'referrer', content: 'no-referrer' }],
    ['meta', { name: 'robots', content: 'index, follow' }],
    ['meta', { property: 'og:site_name', content: 'cvmpay' }],
    ['meta', { property: 'og:image', content: new URL('/og.png', SITE).href }],
    ['meta', { name: 'twitter:card', content: 'summary_large_image' }],
    ['link', { rel: 'alternate', type: 'text/plain', href: '/llms.txt', title: 'llms.txt' }],
  ],
  transformHead({ pageData }): HeadConfig[] {
    if (pageData.isNotFound) return [['meta', { name: 'robots', content: 'noindex' }]];
    const { html, md } = urls(pageData.relativePath);
    const title = pageData.title ? `${pageData.title} · cvmpay docs` : 'cvmpay docs';
    const description = pageData.description || DESCRIPTION;
    const article = {
      '@context': 'https://schema.org',
      '@type': 'TechArticle',
      headline: pageData.title,
      description,
      url: html,
      inLanguage: 'en',
      isPartOf: { '@type': 'WebSite', name: 'cvmpay', url: new URL('/', SITE).href },
      about: { '@type': 'SoftwareApplication', name: 'cvmpay', applicationCategory: 'FinanceApplication' },
      encoding: { '@type': 'MediaObject', encodingFormat: 'text/markdown', contentUrl: new URL(md, SITE).href },
    };
    return [
      ['link', { rel: 'canonical', href: html }],
      ['link', { rel: 'alternate', type: 'text/markdown', href: md, title: 'Markdown version' }],
      ['meta', { property: 'og:type', content: 'article' }],
      ['meta', { property: 'og:title', content: title }],
      ['meta', { property: 'og:description', content: description }],
      ['meta', { property: 'og:url', content: html }],
      ['script', { type: 'application/ld+json' }, JSON.stringify(article).replace(/</g, '\\u003c')],
    ];
  },
  themeConfig: {
    logo: { src: '/favicon.svg', alt: '' },
    siteTitle: 'cvmpay',
    nav: [
      { text: 'Home', link: new URL('/', SITE).href, target: '_self' },
      { text: 'Quick start', link: '/quickstart' },
      { text: 'Security', link: '/security' },
    ],
    sidebar: [
      {
        text: 'Start',
        items: [
          { text: 'Overview', link: '/' },
          { text: 'Quick start', link: '/quickstart' },
          { text: 'How it works', link: '/how-it-works' },
        ],
      },
      {
        text: 'Operate',
        items: [
          { text: 'Configuration', link: '/configuration' },
          { text: 'Deployment', link: '/deployment' },
          { text: 'Tools API', link: '/api' },
        ],
      },
      {
        text: 'Trust',
        items: [
          { text: 'Security', link: '/security' },
          { text: 'Testing', link: '/testing' },
          { text: 'Design decisions', link: '/design' },
          { text: 'Roadmap', link: '/roadmap' },
        ],
      },
      {
        text: 'For agents',
        items: [{ text: 'Markdown and llms.txt', link: '/agents' }],
      },
    ],
    outline: { level: [2, 3] },
    search: { provider: 'local' },
    docFooter: { prev: 'Previous', next: 'Next' },
    footer: { message: 'cvmpay 0.1.0 · Every page is also available as Markdown: replace .html with .md.' },
  },
});
