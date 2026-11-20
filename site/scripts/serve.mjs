// Static preview of dist/ on loopback, with the headers a production host
// should send. Also answers `Accept: text/markdown` with a page's Markdown twin.
// Usage: node scripts/serve.mjs [port]   (default 4330)
import { createServer } from 'node:http';
import { createReadStream, statSync } from 'node:fs';
import { createBrotliCompress, createGzip } from 'node:zlib';
import { extname, join, normalize, resolve, sep, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import { basePath } from './site-url.mjs';

const dist = resolve(dirname(fileURLToPath(import.meta.url)), '..', 'dist');
const port = Number(process.argv[2] ?? process.env.PORT ?? 4330);
// A build for a base path (SITE_URL with a path) is served under that path.
const BASE = basePath();

const TYPES = {
  '.html': 'text/html; charset=utf-8',
  '.md': 'text/markdown; charset=utf-8',
  '.txt': 'text/plain; charset=utf-8',
  '.xml': 'application/xml; charset=utf-8',
  '.json': 'application/json; charset=utf-8',
  '.js': 'text/javascript; charset=utf-8',
  '.css': 'text/css; charset=utf-8',
  '.svg': 'image/svg+xml',
  '.png': 'image/png',
  '.woff2': 'font/woff2',
};

export const SECURITY_HEADERS = {
  'X-Content-Type-Options': 'nosniff',
  'Referrer-Policy': 'no-referrer',
  'Content-Security-Policy': "frame-ancestors 'none'",
  'Cross-Origin-Opener-Policy': 'same-origin',
  'Permissions-Policy': 'camera=(), microphone=(), geolocation=(), payment=()',
};

function file(path) {
  try {
    const s = statSync(path);
    return s.isFile() ? path : null;
  } catch {
    return null;
  }
}

/** Map a URL path to a file inside dist/, or null. Never escapes dist/. */
export function resolvePath(urlPath) {
  let p;
  try {
    p = decodeURIComponent(urlPath);
  } catch {
    return null;
  }
  const target = normalize(join(dist, p));
  if (target !== dist && !target.startsWith(dist + sep)) return null;
  return file(target) ?? file(join(target, 'index.html')) ?? file(`${target}.html`);
}

const server = createServer((req, res) => {
  const url = new URL(req.url ?? '/', 'http://localhost');
  for (const [k, v] of Object.entries(SECURITY_HEADERS)) res.setHeader(k, v);
  if (req.method !== 'GET' && req.method !== 'HEAD') {
    res.writeHead(405, { Allow: 'GET, HEAD' }).end();
    return;
  }
  // The bare base path and /docs get their trailing slash so relative assets resolve.
  if (`${url.pathname}/` === BASE || url.pathname === `${BASE}docs`) {
    res.writeHead(301, { Location: `${url.pathname}/` }).end();
    return;
  }
  let path = url.pathname.startsWith(BASE) ? resolvePath(url.pathname.slice(BASE.length - 1)) : null;
  // Content negotiation: agents asking for Markdown get the .md twin.
  const wantsMd = (req.headers.accept ?? '').includes('text/markdown');
  if (path && wantsMd && path.endsWith('.html')) {
    const md = file(path.replace(/\.html$/, '.md'));
    if (md) path = md;
  }
  res.setHeader('Vary', 'Accept');
  if (!path) {
    res.writeHead(404, { 'Content-Type': TYPES['.html'] });
    createReadStream(join(dist, 'docs', '404.html')).pipe(res);
    return;
  }
  const type = TYPES[extname(path)] ?? 'application/octet-stream';
  const cache = path.includes(`${sep}assets${sep}`) || path.includes(`${sep}_astro${sep}`) ? 'public, max-age=31536000, immutable' : 'no-cache';
  // Compress text like a production host would.
  const accept = String(req.headers['accept-encoding'] ?? '');
  const text = /^(text\/|application\/(json|xml)|image\/svg)/.test(type);
  const encoding = text ? (accept.includes('br') ? 'br' : accept.includes('gzip') ? 'gzip' : null) : null;
  res.writeHead(200, { 'Content-Type': type, 'Cache-Control': cache, ...(encoding ? { 'Content-Encoding': encoding } : {}) });
  if (req.method === 'HEAD') return void res.end();
  const body = createReadStream(path);
  if (encoding === 'br') body.pipe(createBrotliCompress()).pipe(res);
  else if (encoding === 'gzip') body.pipe(createGzip()).pipe(res);
  else body.pipe(res);
});

server.listen(port, '127.0.0.1', () => console.log(`serving ${dist} at http://127.0.0.1:${port}${BASE}`));
