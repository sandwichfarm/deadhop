// What every test page does: load the service config, record events for the test.
import type { Invoice } from '@cvmpay/client';

export async function config(): Promise<{ serverPubkey: string; relays: string[] }> {
  return (await fetch('/config.json')).json();
}

export const events = {
  onCreated: (inv: Invoice): void => document.body.setAttribute('data-created', inv.id),
  onDetected: (inv: Invoice): void => document.body.setAttribute('data-detected', inv.id),
  onPaid: (inv: Invoice): void => document.body.setAttribute('data-paid', inv.id),
};
