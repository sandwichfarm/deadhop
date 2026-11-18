// Hash-based routing: works from any static host, sends nothing to a server.
export type Route = { name: 'home' } | { name: 'pay'; id: string };

export function parseRoute(hash: string): Route {
  const match = /^#\/pay\/([0-9a-f]{32})$/.exec(hash);
  return match ? { name: 'pay', id: match[1] } : { name: 'home' };
}

export function payHash(id: string): string {
  return `#/pay/${id}`;
}
