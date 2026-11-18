// Structured data for the landing page. Every claim here is also visible on
// the page, as search engines and agents expect.
import { benefits, product, pricing } from './content';

export const faq: { q: string; a: string }[] = [
  {
    q: 'Who can create an invoice?',
    a: 'By default only operators, identified by the Nostr public keys you list. You can also allow the public to create invoices; each public request then has to carry proof of work and is rate limited.',
  },
  {
    q: 'Do I log in?',
    a: 'No accounts or passwords. The dashboard signs each request with your Nostr key, from a browser extension or a pasted key.',
  },
  {
    q: 'Can a compromised server spend my bitcoin?',
    a: 'No. The service holds only an account public key (xpub), so it can derive addresses but cannot sign transactions.',
  },
  {
    q: 'What do I need to run it?',
    a: 'The cvmpay binary, a few Nostr relays, and an Electrum server (Fulcrum or electrs) on your own Bitcoin node. The dashboard is a static page you can host anywhere.',
  },
];

export function landingJsonLd(site: URL) {
  const url = new URL('/', site).href;
  return [
    {
      '@context': 'https://schema.org',
      '@type': 'SoftwareApplication',
      name: product.name,
      url,
      description: product.description,
      applicationCategory: 'FinanceApplication',
      applicationSubCategory: 'Bitcoin payment processing',
      operatingSystem: 'Linux, macOS',
      softwareVersion: '0.1.0',
      programmingLanguage: ['Rust', 'TypeScript'],
      featureList: benefits.map((b) => b.title),
      offers: {
        '@type': 'Offer',
        price: '0',
        priceCurrency: 'USD',
        description: pricing,
      },
      softwareHelp: { '@type': 'CreativeWork', url: new URL('/docs/', site).href },
    },
    {
      '@context': 'https://schema.org',
      '@type': 'WebSite',
      name: product.name,
      url,
      description: product.summary,
    },
    {
      '@context': 'https://schema.org',
      '@type': 'FAQPage',
      mainEntity: faq.map((f) => ({
        '@type': 'Question',
        name: f.q,
        acceptedAnswer: { '@type': 'Answer', text: f.a },
      })),
    },
  ];
}
