// Where the site is published, from SITE_URL: an origin
// (https://deadhop.example) or an origin plus a path (GitHub Pages:
// https://sandwichfarm.github.io/deadhop). Used for canonical links, sitemaps,
// Open Graph tags and every absolute link. Local builds fall back to the
// preview server.
export const LOCAL_URL = 'http://127.0.0.1:4330';

function parse() {
  const raw = process.env.SITE_URL?.trim() || LOCAL_URL;
  const url = new URL(raw);
  if (url.search || url.hash) throw new Error(`SITE_URL must be an origin with an optional path, got ${raw}`);
  return url;
}

/** The origin, without a path: https://sandwichfarm.github.io */
export function siteUrl() {
  return parse().origin;
}

/** The path the site lives under, with slashes on both ends: "/" or "/deadhop/". */
export function basePath() {
  const path = parse().pathname.replace(/\/+$/, '');
  return `${path}/`;
}

/** A site path ("/docs/", "llms.txt") under the base path: "/deadhop/docs/". */
export function sitePath(path) {
  return basePath() + path.replace(/^\/+/, '');
}
