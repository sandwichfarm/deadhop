// Props shared by the React components.
import type { CheckoutEvents, Deadhop, Labels } from '@deadhop/client';

export interface CommonProps extends CheckoutEvents {
  /** The service, from `createDeadhop`. Share one per page. */
  client: Pick<Deadhop, 'createInvoice' | 'getInvoice'>;
  /** How often to ask for news, in milliseconds (default 5000). */
  pollMs?: number;
  /** Replace any of the English texts. */
  labels?: Partial<Labels>;
  /** Extra class on the outer element. */
  className?: string;
  /** Copies the address (default: the browser clipboard). */
  copy?: (text: string) => Promise<void>;
}
