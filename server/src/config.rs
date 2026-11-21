//! Configuration, read once at startup from `DEADHOP_*` environment variables.
//!
//! Nothing is read from the network and no value has a network-reaching
//! default: relays, the Electrum backend and the wallet xpub must be supplied
//! (the xpub directly, or as a file written by the offline wallet service).

use std::collections::HashSet;
use std::path::PathBuf;

use bitcoin::Network;
use nostr_sdk::PublicKey;
#[cfg(test)]
use nostr_sdk::ToBech32;

use crate::error::{Error, Result};
use crate::settings::{bounds, Settings, Sweep};

/// Fully validated runtime configuration.
#[derive(Debug, Clone)]
pub struct Config {
    /// Service name reported by `get_info`.
    pub name: String,
    /// Nostr relays used as the only transport.
    pub relays: Vec<String>,
    /// Directory holding the database and the server's nostr key.
    pub data_dir: PathBuf,
    /// Bitcoin network.
    pub network: Network,
    /// Account-level extended public key (watch-only).
    pub xpub: String,
    /// The file the xpub was read from (`DEADHOP_XPUB_FILE`), when it was not
    /// given directly in `DEADHOP_XPUB`.
    pub xpub_file: Option<PathBuf>,
    /// Electrum protocol server, `host:port` (without any `tls://` prefix).
    pub electrum: String,
    /// How the Electrum connection is protected.
    pub electrum_security: crate::electrum::Security,
    /// Hex pubkeys allowed to use admin tools.
    pub admins: HashSet<String>,
    /// Whether non-admin callers may create invoices (PoW gated).
    pub public_invoicing: bool,
    /// NIP-13 difficulty required from non-admin invoice creators.
    pub pow_bits: u8,
    /// Accepted clock skew / age for PoW events, in seconds.
    pub pow_window_secs: u64,
    /// Confirmations needed before an invoice is `confirmed`.
    pub min_confirmations: u32,
    /// Invoice lifetime used when the caller does not choose one.
    pub default_expiry_secs: u64,
    /// Longest invoice lifetime a caller may request.
    pub max_expiry_secs: u64,
    /// Smallest invoice amount.
    pub min_amount_sat: u64,
    /// Largest invoice amount.
    pub max_amount_sat: u64,
    /// Cap on simultaneously open invoices before the public is refused.
    pub max_open_invoices: u64,
    /// Invoices (and therefore address indices) per 24 hours before the public
    /// is refused. This is what bounds address-gap growth.
    pub max_invoices_per_day: u64,
    /// Public invoice creations allowed per minute, all callers together.
    pub rate_global_per_min: u32,
    /// Public invoice creations allowed per minute for one pubkey.
    pub rate_per_key_per_min: u32,
    /// Tool calls allowed per minute from all non-operators together.
    pub rate_requests_per_min: u32,
    /// Tool calls allowed per minute from one non-operator pubkey.
    pub rate_requests_per_key_per_min: u32,
    /// Seconds between chain polls.
    pub poll_secs: u64,
    /// How long an expired invoice keeps being watched for late payment.
    pub late_grace_secs: u64,
    /// Publish public discovery announcements (off by default).
    pub announce: bool,
    /// Relay proxies from the environment.
    pub proxies: crate::settings::ProxySettings,
    /// Forbid changing the proxy settings from the dashboard.
    pub proxy_locked: bool,
}

/// Hard upper bound for any invoice amount: the bitcoin supply cap.
pub const MAX_MONEY_SAT: u64 = 21_000_000 * 100_000_000;

/// Parse a nostr public key given as hex, `npub` or `nprofile`, with or
/// without a `nostr:` prefix. Returns lowercase hex.
pub fn parse_pubkey(s: &str) -> Option<String> {
    let s = s.trim();
    let s = s.strip_prefix("nostr:").unwrap_or(s);
    if let Ok(pk) = PublicKey::parse(s) {
        return Some(pk.to_hex());
    }
    use nostr_sdk::nips::nip19::{FromBech32, Nip19Profile};
    Nip19Profile::from_bech32(s).ok().map(|p| p.public_key.to_hex())
}

/// `host:port` (plain TCP) or `tls://host:port` / `ssl://host:port`, plus
/// optional comma-separated certificate fingerprints to pin.
fn parse_electrum(spec: &str, pins: Option<&str>) -> Result<(String, crate::electrum::Security)> {
    use crate::electrum::{parse_fingerprint, Security};
    let (tls, addr) = match spec.split_once("://") {
        None => (false, spec),
        Some(("tcp", rest)) => (false, rest),
        Some(("tls" | "ssl", rest)) => (true, rest),
        Some(_) => {
            return Err(Error::Config(
                "DEADHOP_ELECTRUM scheme must be tcp://, tls:// or ssl://".into(),
            ))
        }
    };
    let port_ok = addr
        .rsplit_once(':')
        .is_some_and(|(host, port)| !host.is_empty() && port.parse::<u16>().is_ok());
    if !port_ok {
        return Err(Error::Config(
            "DEADHOP_ELECTRUM must be host:port or tls://host:port".into(),
        ));
    }
    let security = match (tls, pins) {
        (false, None) => Security::Plain,
        (false, Some(_)) => {
            return Err(Error::Config(
                "DEADHOP_ELECTRUM_CERT_SHA256 needs a tls:// Electrum server".into(),
            ))
        }
        (true, None) => Security::Tls,
        (true, Some(list)) => {
            let pins = list
                .split(',')
                .map(|p| {
                    parse_fingerprint(p)
                        .ok_or_else(|| Error::Config(format!("'{}' is not a SHA-256 fingerprint", p.trim())))
                })
                .collect::<Result<Vec<_>>>()?;
            Security::Pinned(pins)
        }
    };
    Ok((addr.to_string(), security))
}

/// The account key: `DEADHOP_XPUB`, else the contents of `DEADHOP_XPUB_FILE`
/// (written by `deadhop wallet serve`), with the file it came from.
pub fn account_key(direct: Option<String>, file: Option<String>) -> Result<(String, Option<PathBuf>)> {
    if let Some(key) = direct {
        return Ok((key, None));
    }
    let Some(file) = file else {
        return Err(Error::Config(
            "DEADHOP_XPUB is required, or DEADHOP_XPUB_FILE naming a file that holds it".into(),
        ));
    };
    let path = PathBuf::from(file);
    let key = std::fs::read_to_string(&path)
        .map_err(|e| Error::Config(format!("DEADHOP_XPUB_FILE {}: {e}", path.display())))?
        .trim()
        .to_string();
    if key.is_empty() {
        return Err(Error::Config(format!("DEADHOP_XPUB_FILE {} is empty", path.display())));
    }
    Ok((key, Some(path)))
}

/// Parse a `DEADHOP_NETWORK` value.
pub fn parse_network(s: &str) -> Result<Network> {
    match s {
        "bitcoin" | "mainnet" => Ok(Network::Bitcoin),
        "testnet" => Ok(Network::Testnet),
        "signet" => Ok(Network::Signet),
        "regtest" => Ok(Network::Regtest),
        other => Err(Error::Config(format!("unknown network '{other}'"))),
    }
}

fn parse_bool(key: &str, s: &str) -> Result<bool> {
    match s {
        "1" | "true" | "yes" => Ok(true),
        "0" | "false" | "no" => Ok(false),
        other => Err(Error::Config(format!("{key}: '{other}' is not a boolean"))),
    }
}

/// The relay proxies named in the environment: `DEADHOP_PROXY` (one address,
/// as before) followed by `DEADHOP_PROXIES` (comma-separated), then how to
/// choose among them and what to do when none works.
fn load_proxies(get: &dyn Fn(&str) -> Option<String>) -> Result<crate::settings::ProxySettings> {
    use crate::settings::{bounds, OfflinePolicy, ProxyEntry, ProxyMode, ProxySettings};
    let mut out = ProxySettings::default();
    let single = get("DEADHOP_PROXY").into_iter().map(|u| ("DEADHOP_PROXY", u));
    let many = get("DEADHOP_PROXIES")
        .into_iter()
        .flat_map(|v| {
            v.split(',')
                .map(|u| ("DEADHOP_PROXIES", u.trim().to_string()))
                .collect::<Vec<_>>()
        })
        .filter(|(_, u)| !u.is_empty());
    for (key, url) in single.chain(many) {
        crate::relay::parse_proxy(&url).map_err(|e| Error::Config(format!("{key}: {e}")))?;
        out.list.push(ProxyEntry {
            id: format!("env{}", out.list.len() + 1),
            name: String::new(),
            url,
        });
    }
    let secs = |key: &str, default: u64| -> Result<u64> {
        let Some(s) = get(key) else { return Ok(default) };
        let v = s
            .parse::<u64>()
            .map_err(|_| Error::Config(format!("{key}: '{s}' is not a number")))?;
        let (min, max) = bounds::PROXY_INTERVAL_SECS;
        if v < min || v > max {
            return Err(Error::Config(format!("{key} must be between {min} and {max}")));
        }
        Ok(v)
    };
    out.mode = match get("DEADHOP_PROXY_MODE").as_deref() {
        None | Some("failover") => ProxyMode::Failover,
        Some("round_robin") => ProxyMode::RoundRobin,
        Some("chaos") => ProxyMode::Chaos,
        Some(o) => {
            return Err(Error::Config(format!(
                "DEADHOP_PROXY_MODE: '{o}' is not failover, round_robin or chaos"
            )))
        }
    };
    out.offline = match get("DEADHOP_PROXY_OFFLINE").as_deref() {
        None | Some("kill_switch") => OfflinePolicy::KillSwitch,
        Some("admin_only") => OfflinePolicy::AdminOnly,
        Some("invoicing_only") => OfflinePolicy::InvoicingOnly,
        Some("open") => OfflinePolicy::Open,
        Some(o) => {
            return Err(Error::Config(format!(
                "DEADHOP_PROXY_OFFLINE: '{o}' is not kill_switch, admin_only, invoicing_only or open"
            )))
        }
    };
    out.rotate_secs = secs("DEADHOP_PROXY_ROTATE_SECS", out.rotate_secs)?;
    out.chaos_min_secs = secs("DEADHOP_PROXY_CHAOS_MIN_SECS", out.chaos_min_secs)?;
    out.chaos_max_secs = secs("DEADHOP_PROXY_CHAOS_MAX_SECS", out.chaos_max_secs)?;
    if out.chaos_min_secs > out.chaos_max_secs {
        return Err(Error::Config(
            "DEADHOP_PROXY_CHAOS_MIN_SECS exceeds DEADHOP_PROXY_CHAOS_MAX_SECS".into(),
        ));
    }
    if let Some(v) = get("DEADHOP_TRANSPARENT_PROXY") {
        out.transparent = parse_bool("DEADHOP_TRANSPARENT_PROXY", &v)?;
    }
    out.validate()
        .map_err(|e| Error::Config(format!("DEADHOP_PROXIES: {e}")))?;
    Ok(out)
}

impl Config {
    /// Read configuration from the process environment.
    pub fn from_env() -> Result<Self> {
        Self::from_lookup(|k| std::env::var(k).ok())
    }

    /// Read configuration from any key/value source (used by tests).
    pub fn from_lookup(get: impl Fn(&str) -> Option<String>) -> Result<Self> {
        let get = |k: &str| get(k).map(|v| v.trim().to_string()).filter(|v| !v.is_empty());
        let required = |k: &str| get(k).ok_or_else(|| Error::Config(format!("{k} is required")));
        let num = |k: &str, default: u64, min: u64, max: u64| -> Result<u64> {
            let v = match get(k) {
                None => default,
                Some(s) => s
                    .parse::<u64>()
                    .map_err(|_| Error::Config(format!("{k}: '{s}' is not a number")))?,
            };
            if v < min || v > max {
                return Err(Error::Config(format!("{k} must be between {min} and {max}")));
            }
            Ok(v)
        };
        let flag = |k: &str, default: bool| -> Result<bool> { get(k).map_or(Ok(default), |s| parse_bool(k, &s)) };

        let relays: Vec<String> = required("DEADHOP_RELAYS")?
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        if relays.is_empty() {
            return Err(Error::Config("DEADHOP_RELAYS is required".into()));
        }
        if relays.len() > bounds::RELAYS.1 {
            return Err(Error::Config(format!(
                "DEADHOP_RELAYS lists more than {} relays",
                bounds::RELAYS.1
            )));
        }
        for r in &relays {
            if !(r.starts_with("wss://") || r.starts_with("ws://")) {
                return Err(Error::Config(format!("relay '{r}' must be a ws:// or wss:// url")));
            }
        }

        let (electrum, electrum_security) = parse_electrum(
            &required("DEADHOP_ELECTRUM")?,
            get("DEADHOP_ELECTRUM_CERT_SHA256").as_deref(),
        )?;

        let mut admins = HashSet::new();
        for a in get("DEADHOP_ADMINS").unwrap_or_default().split(',') {
            let a = a.trim();
            if a.is_empty() {
                continue;
            }
            let pk = parse_pubkey(a).ok_or_else(|| Error::Config(format!("admin '{a}' is not a nostr public key")))?;
            admins.insert(pk);
        }

        let public_invoicing = flag("DEADHOP_PUBLIC_INVOICING", false)?;
        if admins.is_empty() && !public_invoicing {
            return Err(Error::Config(
                "set DEADHOP_ADMINS and/or DEADHOP_PUBLIC_INVOICING=true; otherwise nobody could create invoices"
                    .into(),
            ));
        }

        let min_amount_sat = num(
            "DEADHOP_MIN_AMOUNT_SAT",
            1_000,
            bounds::AMOUNT_SAT.0,
            bounds::AMOUNT_SAT.1,
        )?;
        let max_amount_sat = num(
            "DEADHOP_MAX_AMOUNT_SAT",
            100_000_000,
            bounds::AMOUNT_SAT.0,
            bounds::AMOUNT_SAT.1,
        )?;
        if min_amount_sat > max_amount_sat {
            return Err(Error::Config(
                "DEADHOP_MIN_AMOUNT_SAT exceeds DEADHOP_MAX_AMOUNT_SAT".into(),
            ));
        }
        let max_expiry_secs = num(
            "DEADHOP_MAX_EXPIRY_SECS",
            86_400,
            bounds::EXPIRY_SECS.0,
            bounds::EXPIRY_SECS.1,
        )?;
        let default_expiry_secs = num(
            "DEADHOP_DEFAULT_EXPIRY_SECS",
            3_600,
            bounds::EXPIRY_SECS.0,
            bounds::EXPIRY_SECS.1,
        )?;
        if default_expiry_secs > max_expiry_secs {
            return Err(Error::Config(
                "DEADHOP_DEFAULT_EXPIRY_SECS exceeds DEADHOP_MAX_EXPIRY_SECS".into(),
            ));
        }

        let name = get("DEADHOP_NAME").unwrap_or_else(|| "deadhop".into());
        if name.chars().count() > bounds::NAME_LEN || !name.chars().all(crate::service::is_plain_text) {
            return Err(Error::Config(format!(
                "DEADHOP_NAME must be at most {} visible characters",
                bounds::NAME_LEN
            )));
        }

        let (xpub, xpub_file) = account_key(get("DEADHOP_XPUB"), get("DEADHOP_XPUB_FILE"))?;

        Ok(Config {
            name,
            relays,
            data_dir: PathBuf::from(get("DEADHOP_DATA_DIR").unwrap_or_else(|| "data".into())),
            network: parse_network(&get("DEADHOP_NETWORK").unwrap_or_else(|| "bitcoin".into()))?,
            xpub,
            xpub_file,
            electrum,
            electrum_security,
            admins,
            public_invoicing,
            pow_bits: num("DEADHOP_POW_BITS", 20, bounds::POW_BITS.0, bounds::POW_BITS.1)? as u8,
            pow_window_secs: num(
                "DEADHOP_POW_WINDOW_SECS",
                600,
                bounds::POW_WINDOW_SECS.0,
                bounds::POW_WINDOW_SECS.1,
            )?,
            min_confirmations: num(
                "DEADHOP_MIN_CONFIRMATIONS",
                1,
                bounds::MIN_CONFIRMATIONS.0,
                bounds::MIN_CONFIRMATIONS.1,
            )? as u32,
            default_expiry_secs,
            max_expiry_secs,
            min_amount_sat,
            max_amount_sat,
            max_open_invoices: num(
                "DEADHOP_MAX_OPEN_INVOICES",
                500,
                bounds::MAX_OPEN_INVOICES.0,
                bounds::MAX_OPEN_INVOICES.1,
            )?,
            max_invoices_per_day: num(
                "DEADHOP_MAX_INVOICES_PER_DAY",
                2_000,
                bounds::MAX_INVOICES_PER_DAY.0,
                bounds::MAX_INVOICES_PER_DAY.1,
            )?,
            rate_global_per_min: num(
                "DEADHOP_RATE_GLOBAL_PER_MIN",
                30,
                bounds::RATE_CREATE.0,
                bounds::RATE_CREATE.1,
            )? as u32,
            rate_per_key_per_min: num(
                "DEADHOP_RATE_PER_KEY_PER_MIN",
                6,
                bounds::RATE_CREATE.0,
                bounds::RATE_CREATE.1,
            )? as u32,
            rate_requests_per_min: num(
                "DEADHOP_RATE_REQUESTS_PER_MIN",
                1_200,
                bounds::RATE_REQUESTS.0,
                bounds::RATE_REQUESTS.1,
            )? as u32,
            rate_requests_per_key_per_min: num(
                "DEADHOP_RATE_REQUESTS_PER_KEY_PER_MIN",
                120,
                bounds::RATE_REQUESTS.0,
                bounds::RATE_REQUESTS.1,
            )? as u32,
            poll_secs: num("DEADHOP_POLL_SECS", 10, bounds::POLL_SECS.0, bounds::POLL_SECS.1)?,
            late_grace_secs: num(
                "DEADHOP_LATE_GRACE_SECS",
                86_400,
                bounds::LATE_GRACE_SECS.0,
                bounds::LATE_GRACE_SECS.1,
            )?,
            announce: flag("DEADHOP_ANNOUNCE", false)?,
            proxies: load_proxies(&get)?,
            proxy_locked: flag("DEADHOP_PROXY_LOCKED", false)?,
        })
    }
}

/// Whether `host:port` names this machine.
pub fn is_loopback(addr: &str) -> bool {
    is_loopback_host(addr.rsplit_once(':').map_or(addr, |(h, _)| h))
}

/// Whether a bare host (no port; IPv6 with or without brackets) is this
/// machine. Use this on a host that a URL parser extracted, never on a raw
/// authority, which may carry userinfo (`ws://127.0.0.1:80@remote.example`).
pub fn is_loopback_host(host: &str) -> bool {
    let host = host.trim_start_matches('[').trim_end_matches(']');
    host.eq_ignore_ascii_case("localhost") || host.parse::<std::net::IpAddr>().is_ok_and(|ip| ip.is_loopback())
}

impl Config {
    /// Settings that are allowed but that an operator should think twice about.
    pub fn warnings(&self) -> Vec<String> {
        self.settings()
            .warnings(self.network, &self.electrum, &self.electrum_security)
    }

    /// The runtime-changeable part of the configuration, as the environment set it.
    pub fn settings(&self) -> Settings {
        Settings {
            name: self.name.clone(),
            relays: self.relays.clone(),
            public_invoicing: self.public_invoicing,
            pow_bits: self.pow_bits,
            pow_window_secs: self.pow_window_secs,
            min_confirmations: self.min_confirmations,
            default_expiry_secs: self.default_expiry_secs,
            max_expiry_secs: self.max_expiry_secs,
            min_amount_sat: self.min_amount_sat,
            max_amount_sat: self.max_amount_sat,
            max_open_invoices: self.max_open_invoices,
            max_invoices_per_day: self.max_invoices_per_day,
            rate_global_per_min: self.rate_global_per_min,
            rate_per_key_per_min: self.rate_per_key_per_min,
            rate_requests_per_min: self.rate_requests_per_min,
            rate_requests_per_key_per_min: self.rate_requests_per_key_per_min,
            poll_secs: self.poll_secs,
            late_grace_secs: self.late_grace_secs,
            proxies: self.proxies.clone(),
            sweep: Sweep::default(),
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::collections::HashMap;

    pub const ADMIN_HEX: &str = "79be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798";

    pub fn base() -> HashMap<&'static str, String> {
        HashMap::from([
            ("DEADHOP_RELAYS", "ws://127.0.0.1:7777, wss://relay.example".to_string()),
            ("DEADHOP_ELECTRUM", "127.0.0.1:50001".to_string()),
            ("DEADHOP_XPUB", "tpubD6NzVbkrYhZ4XgiXtGrdW5XDAPFCL9h7we1vwNCpn8tGbBcgfVYjXyhWo4E1xkh56hjod1RhGjxbaTLV3X4FyWuejifB9jusQ46QzG87VKp".to_string()),
            ("DEADHOP_NETWORK", "regtest".to_string()),
            ("DEADHOP_ADMINS", ADMIN_HEX.to_string()),
        ])
    }

    pub fn load(m: &HashMap<&'static str, String>) -> Result<Config> {
        Config::from_lookup(|k| m.get(k).cloned())
    }

    fn err_of(m: &HashMap<&'static str, String>) -> String {
        load(m).unwrap_err().to_string()
    }

    #[test]
    fn defaults() {
        let c = load(&base()).unwrap();
        assert_eq!(c.name, "deadhop");
        assert_eq!(c.relays, vec!["ws://127.0.0.1:7777", "wss://relay.example"]);
        assert_eq!(c.network, Network::Regtest);
        assert_eq!(c.data_dir, PathBuf::from("data"));
        assert!(c.admins.contains(ADMIN_HEX));
        assert!(!c.public_invoicing);
        assert!(!c.announce);
        assert_eq!((c.pow_bits, c.pow_window_secs, c.min_confirmations), (20, 600, 1));
        assert_eq!((c.default_expiry_secs, c.max_expiry_secs), (3_600, 86_400));
        assert_eq!((c.min_amount_sat, c.max_amount_sat), (1_000, 100_000_000));
        assert_eq!(
            (c.max_open_invoices, c.rate_global_per_min, c.rate_per_key_per_min),
            (500, 30, 6)
        );
        assert_eq!((c.poll_secs, c.late_grace_secs), (10, 86_400));
        assert_eq!(
            (
                c.max_invoices_per_day,
                c.rate_requests_per_min,
                c.rate_requests_per_key_per_min
            ),
            (2_000, 1_200, 120)
        );
        assert!(c.warnings().is_empty());
    }

    #[test]
    fn loopback_detection() {
        for local in ["127.0.0.1:50001", "localhost:50001", "[::1]:50001", "127.8.9.1:1"] {
            assert!(is_loopback(local), "{local}");
        }
        for remote in [
            "10.0.0.5:50001",
            "electrum.example:50002",
            "[2001:db8::1]:50001",
            "nonsense",
        ] {
            assert!(!is_loopback(remote), "{remote}");
        }
        for local in ["127.0.0.1", "LocalHost", "[::1]", "::1"] {
            assert!(is_loopback_host(local), "{local}");
        }
        for remote in ["relay.example", "[2001:db8::1]", "10.0.0.5", ""] {
            assert!(!is_loopback_host(remote), "{remote}");
        }
    }

    #[test]
    fn the_name_follows_the_dashboard_rules() {
        let mut m = base();
        m.insert("DEADHOP_NAME", "🙂".repeat(60));
        assert_eq!(load(&m).unwrap().name.chars().count(), 60);
        m.insert("DEADHOP_NAME", "x".repeat(61));
        assert!(err_of(&m).contains("DEADHOP_NAME must be at most 60"));
        m.insert("DEADHOP_NAME", "shop\u{202E}".into());
        assert!(err_of(&m).contains("DEADHOP_NAME"));
    }

    #[test]
    fn electrum_over_tls_with_pinned_certificates() {
        use crate::electrum::Security;
        let fp = "ab".repeat(32);
        let mut m = base();
        let c = load(&m).unwrap();
        assert_eq!(
            (c.electrum.as_str(), &c.electrum_security),
            ("127.0.0.1:50001", &Security::Plain)
        );
        m.insert("DEADHOP_ELECTRUM", "tcp://127.0.0.1:50001".into());
        assert_eq!(load(&m).unwrap().electrum_security, Security::Plain);
        m.insert("DEADHOP_ELECTRUM", "tls://node.lan:50002".into());
        let c = load(&m).unwrap();
        assert_eq!(
            (c.electrum.as_str(), &c.electrum_security),
            ("node.lan:50002", &Security::Tls)
        );
        m.insert("DEADHOP_ELECTRUM", "ssl://node.lan:50002".into());
        m.insert("DEADHOP_ELECTRUM_CERT_SHA256", format!("{fp}, {}", "CD".repeat(32)));
        assert_eq!(
            load(&m).unwrap().electrum_security,
            Security::Pinned(vec![[0xab; 32], [0xcd; 32]])
        );
        m.insert("DEADHOP_ELECTRUM_CERT_SHA256", "nope".into());
        assert!(err_of(&m).contains("'nope' is not a SHA-256 fingerprint"));
        m.insert("DEADHOP_ELECTRUM", "node.lan:50001".into());
        m.insert("DEADHOP_ELECTRUM_CERT_SHA256", fp);
        assert!(err_of(&m).contains("needs a tls://"));
        m.remove("DEADHOP_ELECTRUM_CERT_SHA256");
        m.insert("DEADHOP_ELECTRUM", "http://node.lan:1".into());
        assert!(err_of(&m).contains("scheme must be"));
        m.insert("DEADHOP_ELECTRUM", "tls://node.lan".into());
        assert!(err_of(&m).contains("host:port or tls://host:port"));
    }

    #[test]
    fn proxy_comes_from_the_environment() {
        use crate::settings::{OfflinePolicy, ProxyMode};
        let mut m = base();
        let c = load(&m).unwrap();
        assert!(c.proxies.list.is_empty() && !c.proxy_locked && !c.proxies.transparent);
        assert_eq!(
            (c.proxies.mode, c.proxies.offline),
            (ProxyMode::Failover, OfflinePolicy::KillSwitch)
        );
        m.insert("DEADHOP_PROXY", "socks5://127.0.0.1:9050".into());
        m.insert("DEADHOP_PROXY_LOCKED", "true".into());
        let c = load(&m).unwrap();
        assert_eq!(c.proxies.list.len(), 1);
        assert_eq!(
            (c.proxies.list[0].url.as_str(), c.proxy_locked),
            ("socks5://127.0.0.1:9050", true)
        );
        assert_eq!(c.settings().proxies, c.proxies);
        m.insert("DEADHOP_PROXY", "gopher://x:1".into());
        let e = err_of(&m);
        assert!(e.contains("DEADHOP_PROXY") && e.contains("scheme"), "{e}");
        m.insert("DEADHOP_PROXY", "socks5://127.0.0.1:9050".into());
        m.insert("DEADHOP_RELAYS", ["wss://r.example"; 9].join(","));
        assert!(err_of(&m).contains("more than 8"));
    }

    #[test]
    fn several_proxies_and_how_to_use_them_come_from_the_environment() {
        use crate::settings::{OfflinePolicy, ProxyMode};
        let mut m = base();
        m.insert("DEADHOP_PROXY", "socks5://127.0.0.1:9050".into());
        m.insert("DEADHOP_PROXIES", "http://a.lan:3128, socks5://b.lan:1080,".into());
        m.insert("DEADHOP_PROXY_MODE", "chaos".into());
        m.insert("DEADHOP_PROXY_ROTATE_SECS", "120".into());
        m.insert("DEADHOP_PROXY_CHAOS_MIN_SECS", "300".into());
        m.insert("DEADHOP_PROXY_CHAOS_MAX_SECS", "900".into());
        m.insert("DEADHOP_PROXY_OFFLINE", "admin_only".into());
        m.insert("DEADHOP_TRANSPARENT_PROXY", "true".into());
        let p = load(&m).unwrap().proxies;
        assert_eq!(
            p.list
                .iter()
                .map(|e| (e.id.as_str(), e.url.as_str()))
                .collect::<Vec<_>>(),
            [
                ("env1", "socks5://127.0.0.1:9050"),
                ("env2", "http://a.lan:3128"),
                ("env3", "socks5://b.lan:1080")
            ]
        );
        assert_eq!(
            (p.mode, p.rotate_secs, p.offline, p.transparent),
            (ProxyMode::Chaos, 120, OfflinePolicy::AdminOnly, true)
        );
        for (key, bad, needle) in [
            ("DEADHOP_PROXIES", "socks5://a:1,socks5://a:1", "listed twice"),
            ("DEADHOP_PROXIES", "ftp://a:1", "scheme"),
            ("DEADHOP_PROXY_MODE", "random", "failover, round_robin or chaos"),
            ("DEADHOP_PROXY_OFFLINE", "shrug", "kill_switch"),
            ("DEADHOP_PROXY_ROTATE_SECS", "5", "between 60"),
            ("DEADHOP_PROXY_ROTATE_SECS", "soon", "not a number"),
            ("DEADHOP_PROXY_CHAOS_MIN_SECS", "1000", "exceeds"),
            ("DEADHOP_TRANSPARENT_PROXY", "maybe", "not a boolean"),
        ] {
            let mut m = m.clone();
            m.insert(key, bad.into());
            let e = err_of(&m);
            assert!(e.contains(needle), "{key}={bad}: {e}");
        }
    }

    #[test]
    fn operator_keys_may_be_hex_npub_or_nprofile() {
        use nostr_sdk::nips::nip19::{Nip19Profile, ToBech32 as _};
        let keys = nostr_sdk::Keys::generate();
        let hex = keys.public_key().to_hex();
        let npub = keys.public_key().to_bech32().unwrap();
        let profile = Nip19Profile::new(
            keys.public_key(),
            [nostr_sdk::RelayUrl::parse("wss://r.example").unwrap()],
        )
        .to_bech32()
        .unwrap();
        for form in [
            hex.clone(),
            npub.clone(),
            format!("nostr:{npub}"),
            profile.clone(),
            format!(" nostr:{profile} "),
        ] {
            assert_eq!(parse_pubkey(&form).as_deref(), Some(hex.as_str()), "{form}");
        }
        assert_eq!(parse_pubkey("nprofile1nope"), None);
        let mut m = base();
        m.insert("DEADHOP_ADMINS", format!("{profile},{npub}"));
        assert_eq!(load(&m).unwrap().admins.len(), 1);
    }

    #[test]
    fn warns_about_risky_settings() {
        let mut m = base();
        m.insert("DEADHOP_ELECTRUM", "electrum.example:50001".into());
        m.insert("DEADHOP_PUBLIC_INVOICING", "true".into());
        m.insert("DEADHOP_POW_BITS", "0".into());
        m.insert("DEADHOP_NETWORK", "bitcoin".into());
        let w = load(&m).unwrap().warnings();
        assert_eq!(w.len(), 4, "{w:?}");
        assert!(w[0].contains("DEADHOP_ELECTRUM is not on this machine"));
        assert!(w[1].contains("only 0 bits"));
        assert!(w[2].contains("ws://"));
        assert!(w[3].contains("single confirmation"));

        // The same relay list is fine off mainnet; weak work only matters when the public may invoice.
        m.insert("DEADHOP_NETWORK", "regtest".into());
        m.insert("DEADHOP_PUBLIC_INVOICING", "false".into());
        assert_eq!(load(&m).unwrap().warnings().len(), 1);
        m.insert("DEADHOP_NETWORK", "bitcoin".into());
        m.insert("DEADHOP_RELAYS", "wss://relay.example".into());
        m.insert("DEADHOP_ELECTRUM", "127.0.0.1:50001".into());
        m.insert("DEADHOP_MIN_CONFIRMATIONS", "2".into());
        assert!(load(&m).unwrap().warnings().is_empty());
    }

    #[test]
    fn overrides_and_npub_admin() {
        let mut m = base();
        m.insert("DEADHOP_NAME", "shop".into());
        m.insert("DEADHOP_DATA_DIR", "/var/lib/deadhop".into());
        m.insert("DEADHOP_NETWORK", "mainnet".into());
        m.insert("DEADHOP_PUBLIC_INVOICING", "yes".into());
        m.insert("DEADHOP_ANNOUNCE", "1".into());
        m.insert("DEADHOP_POW_BITS", "8".into());
        m.insert(
            "DEADHOP_ADMINS",
            format!(" , {}", PublicKey::parse(ADMIN_HEX).unwrap().to_bech32().unwrap()),
        );
        let c = load(&m).unwrap();
        assert_eq!(c.name, "shop");
        assert_eq!(c.data_dir, PathBuf::from("/var/lib/deadhop"));
        assert_eq!(c.network, Network::Bitcoin);
        assert!(c.public_invoicing && c.announce);
        assert_eq!(c.pow_bits, 8);
        assert_eq!(c.admins.len(), 1);
        assert!(c.admins.contains(ADMIN_HEX));
    }

    #[test]
    fn networks() {
        for (s, n) in [
            ("bitcoin", Network::Bitcoin),
            ("testnet", Network::Testnet),
            ("signet", Network::Signet),
            ("regtest", Network::Regtest),
        ] {
            let mut m = base();
            m.insert("DEADHOP_NETWORK", s.into());
            assert_eq!(load(&m).unwrap().network, n);
        }
        // Mainnet is the default when the variable is absent.
        let mut m = base();
        m.remove("DEADHOP_NETWORK");
        assert_eq!(load(&m).unwrap().network, Network::Bitcoin);
        let mut m = base();
        m.insert("DEADHOP_NETWORK", "dogecoin".into());
        assert!(err_of(&m).contains("unknown network"));
    }

    #[test]
    fn the_xpub_can_come_from_a_file() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("account.xpub");
        let mut m = base();
        let key = m.remove("DEADHOP_XPUB").unwrap();
        m.insert("DEADHOP_XPUB_FILE", file.to_string_lossy().into_owned());
        assert!(err_of(&m).contains("account.xpub"), "a missing file is named");
        std::fs::write(&file, " \n").unwrap();
        assert!(err_of(&m).contains("is empty"));
        std::fs::write(&file, format!("{key}\n")).unwrap();
        let c = load(&m).unwrap();
        assert_eq!(c.xpub, key);
        assert_eq!(c.xpub_file.as_deref(), Some(file.as_path()));
        // A key given directly wins over the file.
        m.insert("DEADHOP_XPUB", key.clone());
        assert_eq!(load(&m).unwrap().xpub_file, None);
    }

    #[test]
    fn required_values() {
        for k in ["DEADHOP_RELAYS", "DEADHOP_ELECTRUM", "DEADHOP_XPUB"] {
            let mut m = base();
            m.remove(k);
            assert!(err_of(&m).contains(&format!("{k} is required")), "{k}");
            m.insert(k, "   ".into());
            assert!(err_of(&m).contains(&format!("{k} is required")), "{k} blank");
        }
        let mut m = base();
        m.insert("DEADHOP_RELAYS", " , ,".into());
        assert!(err_of(&m).contains("DEADHOP_RELAYS is required"));
    }

    #[test]
    fn rejects_bad_values() {
        let cases = [
            ("DEADHOP_RELAYS", "https://relay.example", "must be a ws://"),
            ("DEADHOP_ELECTRUM", "localhost", "host:port"),
            ("DEADHOP_ELECTRUM", ":50001", "host:port"),
            ("DEADHOP_ELECTRUM", "host:notaport", "host:port"),
            ("DEADHOP_ADMINS", "nope", "not a nostr public key"),
            ("DEADHOP_PUBLIC_INVOICING", "maybe", "not a boolean"),
            ("DEADHOP_POW_BITS", "many", "not a number"),
            ("DEADHOP_POW_BITS", "65", "between 0 and 64"),
            ("DEADHOP_MIN_CONFIRMATIONS", "0", "between 1 and 144"),
            ("DEADHOP_MIN_AMOUNT_SAT", "200000000", "exceeds DEADHOP_MAX_AMOUNT_SAT"),
            (
                "DEADHOP_DEFAULT_EXPIRY_SECS",
                "100000",
                "exceeds DEADHOP_MAX_EXPIRY_SECS",
            ),
        ];
        for (k, v, want) in cases {
            let mut m = base();
            m.insert(k, v.into());
            let e = err_of(&m);
            assert!(e.contains(want), "{k}={v}: {e}");
        }
    }

    #[test]
    fn needs_someone_who_can_invoice() {
        let mut m = base();
        m.remove("DEADHOP_ADMINS");
        assert!(err_of(&m).contains("nobody could create invoices"));
        m.insert("DEADHOP_PUBLIC_INVOICING", "true".into());
        assert!(load(&m).unwrap().admins.is_empty());
        m.insert("DEADHOP_PUBLIC_INVOICING", "false".into());
        assert!(err_of(&m).contains("nobody could create invoices"));
        m.insert("DEADHOP_PUBLIC_INVOICING", "no".into());
        m.insert("DEADHOP_ADMINS", ADMIN_HEX.into());
        m.insert("DEADHOP_ANNOUNCE", "0".into());
        assert!(!load(&m).unwrap().public_invoicing);
    }

    #[test]
    fn from_env_reads_process_environment() {
        // No DEADHOP_* variables are set in the test process.
        assert!(Config::from_env()
            .unwrap_err()
            .to_string()
            .contains("DEADHOP_RELAYS is required"));
    }
}
