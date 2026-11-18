// Create a nostr key pair for local development: keygen.mjs <path-prefix>
// Writes <prefix>.nsec (owner-only) and <prefix>.pubkey (hex).
import { writeFileSync } from 'node:fs';
import { generateSecretKey, getPublicKey, nip19 } from 'nostr-tools';

const prefix = process.argv[2];
if (!prefix) {
  console.error('usage: keygen.mjs <path-prefix>');
  process.exit(1);
}
const secret = generateSecretKey();
writeFileSync(`${prefix}.nsec`, nip19.nsecEncode(secret), { mode: 0o600, flag: 'wx' });
writeFileSync(`${prefix}.pubkey`, getPublicKey(secret));
console.log(`wrote ${prefix}.nsec and ${prefix}.pubkey`);
