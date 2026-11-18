// Markdown version of the landing page, for agents (linked with rel="alternate").
import type { APIRoute } from 'astro';
import { benefits, components, docLinks, limits, pricing, product, steps } from '../content';
import { faq } from '../seo';

export const GET: APIRoute = () => {
  const out = [
    `# ${product.name}`,
    '',
    `> ${product.tagline}`,
    '',
    product.summary,
    '',
    '## How a payment flows',
    '',
    ...steps.flatMap((s) => [`### ${s.title}`, '', s.body, '']),
    '## Security benefits',
    '',
    ...benefits.map((b) => `- **${b.title}.** ${b.body}`),
    '',
    '## What it does not protect against',
    '',
    ...limits.map((l) => `- ${l}`),
    '',
    '## What you run',
    '',
    ...components.map((c) => `- **${c.name}:** ${c.body}`),
    '',
    `**Price:** ${pricing}`,
    '',
    '## Questions',
    '',
    ...faq.flatMap((f) => [`**${f.q}**`, '', f.a, '']),
    '## Documentation',
    '',
    ...docLinks.map((d) => `- [${d.title}](${d.href.replace(/\.html$/, '.md')}): ${d.body}`),
    '',
  ].join('\n');
  return new Response(out, { headers: { 'Content-Type': 'text/markdown; charset=utf-8' } });
};
