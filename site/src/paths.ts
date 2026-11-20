// The site may be published under a base path (GitHub Pages:
// /deadhop/). Write site paths as "/docs/" and pass them through `at`.
const base = import.meta.env.BASE_URL.replace(/\/?$/, '/');

/** "/docs/" -> "/deadhop/docs/" (or "/docs/" at the root of a domain). */
export const at = (path: string): string => base + path.replace(/^\/+/, '');
