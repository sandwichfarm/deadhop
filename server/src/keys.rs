//! The service's nostr keys, one per purpose.
//!
//! What each key may do is fixed here, in one table. Settings refer to keys by
//! their `id` (a string), so a later key with a narrower scope is one more row
//! in [`KEYS`] and needs no change to anything saved.

use std::path::Path;

use nostr_sdk::Keys;

use crate::error::{Error, Result};

/// The key the public reaches: invoices and nothing else.
pub const INVOICES: &str = "invoices";
/// The key operators manage the service through.
pub const ADMIN: &str = "admin";

/// One key and its fixed scope.
#[derive(Debug, PartialEq, Eq)]
pub struct KeyDef {
    pub id: &'static str,
    /// What the dashboard calls it.
    pub label: &'static str,
    /// File (in the data directory) holding the secret key.
    pub file: &'static str,
    /// File the public key is written to once the service is serving.
    pub pubkey_file: &'static str,
    /// The tools this key serves; `None` is all of them.
    pub tools: Option<&'static [&'static str]>,
    /// Only operators may use this key at all.
    pub operators_only: bool,
    /// Whether this key may be announced on the relays (when announcing is on).
    pub announce: bool,
}

/// The tools anyone may call.
pub const PUBLIC_TOOLS: [&str; 3] = ["get_info", "create_invoice", "get_invoice"];

/// Every key, in the order they are shown and started.
pub const KEYS: [KeyDef; 2] = [
    KeyDef {
        id: INVOICES,
        label: "Public invoices",
        // The first key the service ever had: it stays the public one, so the
        // address customers already use does not change.
        file: crate::app::KEY_FILE,
        pubkey_file: crate::app::PUBKEY_FILE,
        tools: Some(&PUBLIC_TOOLS),
        operators_only: false,
        announce: true,
    },
    KeyDef {
        id: ADMIN,
        label: "Administration",
        file: "admin.nsec",
        pubkey_file: "admin.pubkey",
        tools: None,
        operators_only: true,
        announce: false,
    },
];

/// The key with this id.
pub fn def(id: &str) -> Option<&'static KeyDef> {
    KEYS.iter().find(|k| k.id == id)
}

/// Whether a list of key ids assigns something to `id`. An empty list is
/// every key.
pub fn assigned(keys: &[String], id: &str) -> bool {
    keys.is_empty() || keys.iter().any(|k| k == id)
}

/// Check that every id names a key, and none is named twice.
pub fn check_ids(keys: &[String], what: &str) -> Result<()> {
    for (i, k) in keys.iter().enumerate() {
        if def(k).is_none() {
            return Err(Error::InvalidParams(format!("{what}: there is no key '{k}'")));
        }
        if keys[..i].contains(k) {
            return Err(Error::InvalidParams(format!("{what}: key '{k}' is named twice")));
        }
    }
    Ok(())
}

/// A key and what it is for.
pub struct Identity {
    pub def: &'static KeyDef,
    pub keys: Keys,
}

impl Identity {
    pub fn pubkey(&self) -> String {
        self.keys.public_key().to_hex()
    }
}

/// Load every key from the data directory, creating the ones that do not exist
/// yet (the data directory too).
pub fn open_identities(data_dir: &Path) -> Result<Vec<Identity>> {
    crate::app::open_dir(data_dir)?;
    KEYS.iter()
        .map(|def| {
            Ok(Identity {
                def,
                keys: crate::app::load_or_create_keys(&data_dir.join(def.file))?,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_table_is_consistent() {
        assert_eq!(def(INVOICES).unwrap().file, "server.nsec");
        assert!(def("nobody").is_none());
        let ids: Vec<_> = KEYS.iter().map(|k| k.id).collect();
        assert_eq!(ids, [INVOICES, ADMIN]);
        // Nothing but the public tools reaches the public key, and the admin
        // key is never announced.
        assert_eq!(def(INVOICES).unwrap().tools, Some(&PUBLIC_TOOLS[..]));
        assert!(!def(ADMIN).unwrap().announce && def(ADMIN).unwrap().operators_only);
        assert!(!def(INVOICES).unwrap().operators_only);
    }

    #[test]
    fn an_empty_assignment_is_every_key() {
        let only = vec![ADMIN.to_string()];
        assert!(assigned(&[], INVOICES) && assigned(&[], ADMIN));
        assert!(assigned(&only, ADMIN) && !assigned(&only, INVOICES));
    }

    #[test]
    fn ids_are_checked() {
        assert!(check_ids(&[], "x").is_ok());
        assert!(check_ids(&[ADMIN.into(), INVOICES.into()], "x").is_ok());
        let unknown = check_ids(&["root".into()], "relay").unwrap_err().to_string();
        assert!(unknown.contains("no key 'root'"), "{unknown}");
        let twice = check_ids(&[ADMIN.into(), ADMIN.into()], "proxy")
            .unwrap_err()
            .to_string();
        assert!(twice.contains("twice"), "{twice}");
    }

    #[test]
    fn each_key_is_created_once_and_kept() {
        let dir = tempfile::tempdir().unwrap();
        let first = open_identities(&dir.path().join("data")).unwrap();
        let again = open_identities(&dir.path().join("data")).unwrap();
        assert_eq!(first.len(), 2);
        assert_ne!(first[0].pubkey(), first[1].pubkey());
        assert_eq!(first[0].pubkey(), again[0].pubkey());
        assert_eq!(first[1].pubkey(), again[1].pubkey());
        // A file in the way of a key is an error, not a new key.
        let file = dir.path().join("plain");
        std::fs::write(&file, "x").unwrap();
        assert!(open_identities(&file).is_err());
    }
}
