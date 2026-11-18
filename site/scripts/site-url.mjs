// The public origin, used for canonical links, sitemaps and Open Graph tags.
// Production builds must set SITE_URL; local builds fall back to the preview server.
export const LOCAL_URL = 'http://127.0.0.1:4330';

export function siteUrl() {
  const raw = process.env.SITE_URL?.trim() || LOCAL_URL;
  const url = new URL(raw);
  if (url.pathname !== '/' || url.search || url.hash) throw new Error(`SITE_URL must be an origin, got ${raw}`);
  return url.origin;
}
