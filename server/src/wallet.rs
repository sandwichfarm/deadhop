//! Watch-only wallet: derives one fresh native-segwit address per invoice from
//! an account-level extended **public** key. The payment service never holds
//! a private key, so compromising it cannot spend funds. (The offline wallet
//! service in `keyholder` uses `account_from_phrase` once, to derive the key
//! it hands over; it has no network access.)

use std::str::FromStr;

use bitcoin::bip32::{ChildNumber, DerivationPath, Xpriv, Xpub};
use bitcoin::hashes::{sha256, Hash};
use bitcoin::secp256k1::{Secp256k1, VerifyOnly};
use bitcoin::{base58, Address, CompressedPublicKey, Network, NetworkKind, ScriptBuf};

use crate::error::{Error, Result};

const XPUB_VERSION: [u8; 4] = [0x04, 0x88, 0xB2, 0x1E];
const TPUB_VERSION: [u8; 4] = [0x04, 0x35, 0x87, 0xCF];
const ZPUB_VERSION: [u8; 4] = [0x04, 0xB2, 0x47, 0x46];
const VPUB_VERSION: [u8; 4] = [0x04, 0x5F, 0x1C, 0xF6];

/// Highest usable non-hardened child index.
pub const MAX_INDEX: u32 = (1 << 31) - 1;

/// Address deriver for the external chain (`<account>/0/*`).
pub struct Wallet {
    secp: Secp256k1<VerifyOnly>,
    external: Xpub,
    network: Network,
}

impl Wallet {
    /// Parse an `xpub`/`tpub` (or SLIP-132 `zpub`/`vpub`) for `network`.
    pub fn new(xpub: &str, network: Network) -> Result<Self> {
        let bad = |why: &str| Error::Config(format!("DEADHOP_XPUB: {why}"));
        let mut raw = base58::decode_check(xpub).map_err(|_| bad("not a valid extended public key"))?;
        if raw.len() != 78 {
            return Err(bad("not a valid extended public key"));
        }
        // SLIP-132 zpub/vpub carry the same key material as xpub/tpub.
        if raw[..4] == ZPUB_VERSION {
            raw[..4].copy_from_slice(&XPUB_VERSION);
        } else if raw[..4] == VPUB_VERSION {
            raw[..4].copy_from_slice(&TPUB_VERSION);
        }
        let account = Xpub::decode(&raw).map_err(|_| bad("not a valid extended public key"))?;
        let want = NetworkKind::from(network);
        if account.network != want {
            return Err(bad("key belongs to a different network than DEADHOP_NETWORK"));
        }
        let secp = Secp256k1::verification_only();
        // Non-hardened derivation fails only if the tweak is out of range,
        // which happens with probability about 2^-127 (BIP-32).
        let external = account
            .derive_pub(&secp, &[ChildNumber::Normal { index: 0 }])
            .expect("non-hardened derivation from a valid key");
        Ok(Wallet {
            secp,
            external,
            network,
        })
    }

    /// Stable identifier of the account key (whatever its encoding), so a
    /// database can refuse to be reused with a different wallet.
    pub fn account_id(&self) -> String {
        hex::encode(sha256::Hash::hash(&self.external.encode()).to_byte_array())
    }

    /// P2WPKH address at `<account>/0/index`.
    pub fn address(&self, index: u32) -> Result<Address> {
        if index > MAX_INDEX {
            return Err(Error::Capacity);
        }
        let child = self
            .external
            .derive_pub(&self.secp, &[ChildNumber::Normal { index }])
            .expect("non-hardened derivation from a valid key");
        Ok(Address::p2wpkh(&CompressedPublicKey(child.public_key), self.network))
    }

    /// Parse an address previously produced by this wallet.
    pub fn script_for(&self, address: &str) -> Result<ScriptBuf> {
        let a = Address::from_str(address)
            .map_err(|e| Error::Chain(format!("stored address invalid: {e}")))?
            .require_network(self.network)
            .map_err(|e| Error::Chain(format!("stored address invalid: {e}")))?;
        Ok(a.script_pubkey())
    }
}

/// BIP-84 account path: coin type 0 on mainnet, 1 on every test network.
pub fn account_path(network: Network) -> &'static str {
    match network {
        Network::Bitcoin => "m/84'/0'/0'",
        _ => "m/84'/1'/0'",
    }
}

/// Account public key of a BIP-39 recovery phrase (no passphrase), BIP-84.
/// The private keys exist only for the duration of this call.
pub fn account_from_phrase(phrase: &bip39::Mnemonic, network: Network) -> Xpub {
    let secp = Secp256k1::new();
    let seed = phrase.to_seed("");
    let master = Xpriv::new_master(network, &seed).expect("a 64-byte seed is a valid master key");
    let path = DerivationPath::from_str(account_path(network)).expect("constant path");
    let account = master
        .derive_priv(&secp, &path)
        .expect("hardened derivation from a valid key");
    Xpub::from_priv(&secp, &account)
}

/// SLIP-132 encoding of an account key, as wallets show BIP-84 keys:
/// `zpub` on mainnet, `vpub` on test networks.
pub fn encode_account(account: &Xpub) -> String {
    let mut raw = account.encode().to_vec();
    let version = match account.network {
        NetworkKind::Main => ZPUB_VERSION,
        NetworkKind::Test => VPUB_VERSION,
    };
    raw[..4].copy_from_slice(&version);
    base58::encode_check(&raw)
}

/// Electrum protocol script hash: `sha256(scriptPubKey)`, byte-reversed, hex.
pub fn scripthash(script: &ScriptBuf) -> String {
    let mut h = sha256::Hash::hash(script.as_bytes()).to_byte_array();
    h.reverse();
    hex::encode(h)
}

/// BIP-21 payment URI for `address` and `amount_sat`.
pub fn bip21(address: &str, amount_sat: u64) -> String {
    format!(
        "bitcoin:{address}?amount={}.{:08}",
        amount_sat / 100_000_000,
        amount_sat % 100_000_000
    )
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    // BIP-84 test vector: mnemonic "abandon ... about" (BIP-39 seed below),
    // account m/84'/0'/0'. Expected addresses are the ones published in BIP-84.
    const SEED: &str = "5eb00bbddcf069084889a8ab9155568165f5c453ccb85e70811aaed6f6da5fc19a5ac40b389cd370d086206dec8aa6c43daea6690f20ad3d8d48b2d2ce9e38e4";

    /// Account xpub of the BIP-84 vector, encoded with the given version bytes.
    fn vector_key(version: [u8; 4]) -> String {
        use bitcoin::bip32::{DerivationPath, Xpriv};
        let secp = Secp256k1::new();
        let master = Xpriv::new_master(Network::Bitcoin, &hex::decode(SEED).unwrap()).unwrap();
        let path = DerivationPath::from_str("m/84'/0'/0'").unwrap();
        let account = Xpub::from_priv(&secp, &master.derive_priv(&secp, &path).unwrap());
        let mut raw = account.encode().to_vec();
        raw[..4].copy_from_slice(&version);
        base58::encode_check(&raw)
    }
    pub const TPUB: &str = "tpubD6NzVbkrYhZ4XgiXtGrdW5XDAPFCL9h7we1vwNCpn8tGbBcgfVYjXyhWo4E1xkh56hjod1RhGjxbaTLV3X4FyWuejifB9jusQ46QzG87VKp";

    #[test]
    fn bip84_vectors() {
        for (key, prefix) in [(vector_key(ZPUB_VERSION), "zpub"), (vector_key(XPUB_VERSION), "xpub")] {
            assert!(key.starts_with(prefix));
            let w = Wallet::new(&key, Network::Bitcoin).unwrap();
            assert_eq!(
                w.address(0).unwrap().to_string(),
                "bc1qcr8te4kr609gcawutmrza0j4xv80jy8z306fyu"
            );
            assert_eq!(
                w.address(1).unwrap().to_string(),
                "bc1qnjg0jd8228aq7egyzacy8cys3knf9xvrerkf9g"
            );
        }
    }

    #[test]
    fn a_recovery_phrase_derives_the_bip84_account() {
        // The BIP-84 test vector's mnemonic.
        let words = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";
        let phrase = bip39::Mnemonic::parse(words).unwrap();
        assert_eq!(hex::encode(phrase.to_seed("")), SEED);
        let account = account_from_phrase(&phrase, Network::Bitcoin);
        let zpub = encode_account(&account);
        assert_eq!(zpub, vector_key(ZPUB_VERSION));
        let w = Wallet::new(&zpub, Network::Bitcoin).unwrap();
        assert_eq!(
            w.address(0).unwrap().to_string(),
            "bc1qcr8te4kr609gcawutmrza0j4xv80jy8z306fyu"
        );
        // The same key in xpub encoding is the same account.
        assert_eq!(
            w.account_id(),
            Wallet::new(&vector_key(XPUB_VERSION), Network::Bitcoin)
                .unwrap()
                .account_id()
        );
        assert_eq!(account_path(Network::Bitcoin), "m/84'/0'/0'");

        let test = account_from_phrase(&phrase, Network::Regtest);
        assert_eq!(account_path(Network::Regtest), "m/84'/1'/0'");
        let vpub = encode_account(&test);
        assert!(vpub.starts_with("vpub"), "{vpub}");
        let t = Wallet::new(&vpub, Network::Regtest).unwrap();
        assert!(t.address(0).unwrap().to_string().starts_with("bcrt1q"));
        assert_ne!(t.account_id(), w.account_id());
    }

    #[test]
    fn regtest_addresses_and_vpub() {
        let w = Wallet::new(TPUB, Network::Regtest).unwrap();
        let a0 = w.address(0).unwrap().to_string();
        assert!(a0.starts_with("bcrt1q"), "{a0}");
        assert_ne!(a0, w.address(1).unwrap().to_string());
        assert_eq!(w.script_for(&a0).unwrap(), w.address(0).unwrap().script_pubkey());

        // The same key re-encoded with the SLIP-132 vpub prefix derives identically.
        let mut raw = base58::decode_check(TPUB).unwrap();
        raw[..4].copy_from_slice(&VPUB_VERSION);
        let vpub = base58::encode_check(&raw);
        assert!(vpub.starts_with("vpub"));
        let t = Wallet::new(&vpub, Network::Testnet).unwrap();
        assert!(t.address(0).unwrap().to_string().starts_with("tb1q"));
        assert_eq!(
            t.address(0).unwrap().script_pubkey(),
            w.address(0).unwrap().script_pubkey()
        );
    }

    #[test]
    fn rejects_bad_keys() {
        let e = |k: &str, n| Wallet::new(k, n).err().unwrap().to_string();
        assert!(e("not-a-key", Network::Bitcoin).contains("not a valid extended public key"));
        assert!(e(&base58::encode_check(&[1, 2, 3]), Network::Bitcoin).contains("not a valid"));
        assert!(e(&base58::encode_check(&[0u8; 78]), Network::Bitcoin).contains("not a valid"));
        assert!(e(&vector_key(XPUB_VERSION), Network::Regtest).contains("different network"));
        assert!(e(TPUB, Network::Bitcoin).contains("different network"));
    }

    #[test]
    fn index_bounds_and_foreign_addresses() {
        let w = Wallet::new(TPUB, Network::Regtest).unwrap();
        assert!(w.address(MAX_INDEX).is_ok());
        assert!(matches!(w.address(MAX_INDEX + 1), Err(Error::Capacity)));
        assert!(w.script_for("garbage").is_err());
        assert!(w.script_for("bc1qcr8te4kr609gcawutmrza0j4xv80jy8z306fyu").is_err());
    }

    #[test]
    fn scripthash_and_bip21() {
        // Electrum protocol documentation example (genesis coinbase P2PKH address).
        let a = Address::from_str("1A1zP1eP5QGefi2DMPTfTL5SLmv7DivfNa")
            .unwrap()
            .assume_checked();
        assert_eq!(
            scripthash(&a.script_pubkey()),
            "8b01df4e368ea28f8dc0423bcf7a4923e3a12d307c875e47a0cfbf90b5c39161"
        );
        assert_eq!(bip21("bc1qx", 1), "bitcoin:bc1qx?amount=0.00000001");
        assert_eq!(bip21("bc1qx", 250_000_000), "bitcoin:bc1qx?amount=2.50000000");
    }
}
