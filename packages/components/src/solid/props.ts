// Props shared by the Solid components.
import type { CheckoutEvents, Cvmpay, Labels } from '@cvmpay/client';

export interface CommonProps extends CheckoutEvents {
  /** The service, from `createCvmpay`. Share one per page. */
  client: Pick<Cvmpay, 'createInvoice' | 'getInvoice'>;
  /** How often to ask for news, in milliseconds (default 5000). */
  pollMs?: number;
  /** Replace any of the English texts. */
  labels?: Partial<Labels>;
  /** Extra class on the outer element. */
  class?: string;
  /** Copies the address (default: the browser clipboard). */
  copy?: (text: string) => Promise<void>;
}
