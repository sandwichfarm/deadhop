// @ts-check
import { defineConfig } from 'astro/config';
import { basePath, siteUrl } from './scripts/site-url.mjs';

export default defineConfig({
  site: siteUrl(),
  base: basePath(),
  output: 'static',
  trailingSlash: 'ignore',
  build: { format: 'directory', inlineStylesheets: 'never' },
  // Keep the CSP strict: no inline scripts in the built page.
  vite: { build: { assetsInlineLimit: 0 } },
  devToolbar: { enabled: false },
});
