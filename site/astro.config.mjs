// @ts-check
import { defineConfig } from 'astro/config';
import { siteUrl } from './scripts/site-url.mjs';

export default defineConfig({
  site: siteUrl(),
  output: 'static',
  trailingSlash: 'ignore',
  build: { format: 'directory', inlineStylesheets: 'never' },
  // Keep the CSP strict: no inline scripts in the built page.
  vite: { build: { assetsInlineLimit: 0 } },
  devToolbar: { enabled: false },
});
