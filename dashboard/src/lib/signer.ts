// Nostr identities used instead of a login. Nothing is ever persisted.
import { PrivateKeySigner, type NostrSigner } from '@contextvm/sdk';
import { nip19 } from 'nostr-tools';
import { bytesToHex } from '@noble/hashes/utils.js';

/** The subset of a NIP-07 browser extension this app relies on. */
export interface Nip07 {
  getPublicKey(): Promise<string>;
  signEvent(event: object): Promise<unknown>;
  nip44?: {
    encrypt(pubkey: string, plaintext: string): Promise<string>;
    decrypt(pubkey: string, ciphertext: string): Promise<string>;
  };
}

/** Sign with a browser extension; the key never enters this page. */
export function nip07Signer(ext: Nip07 | undefined): NostrSigner {
  if (!ext) throw new Error('No Nostr extension found in this browser');
  const nip44 = ext.nip44;
  if (!nip44) throw new Error('This Nostr extension does not support NIP-44 encryption');
  return {
    getPublicKey: () => ext.getPublicKey(),
    signEvent: (event) => ext.signEvent(event) as ReturnType<NostrSigner['signEvent']>,
    nip44: {
      encrypt: (pubkey, plaintext) => nip44.encrypt(pubkey, plaintext),
      decrypt: (pubkey, ciphertext) => nip44.decrypt(pubkey, ciphertext),
    },
  };
}

/** Parse an `nsec` or 64-character hex secret key into hex. */
export function parseSecret(input: string): string {
  const v = input.trim();
  if (/^[0-9a-fA-F]{64}$/.test(v)) return v.toLowerCase();
  try {
    const decoded = nip19.decode(v);
    if (decoded.type === 'nsec') return bytesToHex(decoded.data);
  } catch {
    // fall through to the shared error below
  }
  throw new Error('Enter an nsec or a 64-character hex secret key');
}

/** Sign with a key held only in memory. Without a secret, a throwaway key. */
export function localSigner(secret?: string): NostrSigner {
  return secret === undefined ? new PrivateKeySigner() : new PrivateKeySigner(parseSecret(secret));
}
