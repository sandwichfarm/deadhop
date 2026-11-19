// Named signers for the two common cases.
import type { NostrSigner } from '@contextvm/sdk';
import { localSigner } from './signer.ts';

/** A throwaway key made in memory: what a customer's browser uses. */
export function guestSigner(): NostrSigner {
  return localSigner();
}

/**
 * Sign with a secret key (`nsec` or hex), for example an operator key on your
 * backend. Never ship an operator key to a browser: it can manage the service.
 */
export function secretKeySigner(secret: string): NostrSigner {
  return localSigner(secret);
}
