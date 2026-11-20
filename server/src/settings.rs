//! Settings an operator may change at runtime from the dashboard.
//!
//! The environment supplies the defaults; changes made remotely are stored as
//! overrides in the database and win over the environment until reset.
//! Anything that decides where money goes or whom the service trusts (the
//! xpub, network, Electrum server, operator list, data directory) is
//! deliberately not here: a stolen operator key must not be able to redirect
//! payments or lock the owner out.

use bitcoin::Network;
use rmcp::schemars::{self, JsonSchema};
use serde::{Deserialize, Serialize};

use crate::config::{is_loopback, is_loopback_host, MAX_MONEY_SAT};
use crate::error::{Error, Result};

/// Inclusive bounds shared by the environment loader and runtime changes, so
/// the two can never disagree about what is valid.
pub mod bounds {
    pub const POW_BITS: (u64, u64) = (0, 64);
    pub const POW_WINDOW_SECS: (u64, u64) = (30, 3_600);
    pub const MIN_CONFIRMATIONS: (u64, u64) = (1, 144);
    pub const EXPIRY_SECS: (u64, u64) = (60, 30 * 86_400);
    pub const AMOUNT_SAT: (u64, u64) = (1, super::MAX_MONEY_SAT);
    pub const MAX_OPEN_INVOICES: (u64, u64) = (1, 100_000);
    pub const MAX_INVOICES_PER_DAY: (u64, u64) = (1, 1_000_000);
    pub const RATE_CREATE: (u64, u64) = (1, 100_000);
    pub const RATE_REQUESTS: (u64, u64) = (1, 1_000_000);
    pub const POLL_SECS: (u64, u64) = (1, 3_600);
    pub const LATE_GRACE_SECS: (u64, u64) = (0, 30 * 86_400);
    pub const RELAYS: (usize, usize) = (1, 8);
    pub const RELAY_URL_LEN: usize = 200;
    pub const NAME_LEN: usize = 60;
    pub const SWEEP_THRESHOLD_SAT: (u64, u64) = (10_000, super::MAX_MONEY_SAT);
    pub const SWEEP_FEE_RATE: (u64, u64) = (1, 1_000);
    pub const SWEEP_ANONYMITY_SET: (u64, u64) = (2, 1_000);
}

/// How a future sweep would leave this wallet.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum SweepMode {
    Payjoin,
    Coinjoin,
}

/// Policy for moving funds out once they pile up. Stored and reported only:
/// this service holds no spending keys, so a separate signer must carry it
/// out. There is intentionally no destination field; the signer owns that.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Sweep {
    /// Whether the policy is active.
    pub enabled: bool,
    /// PayJoin or CoinJoin.
    pub mode: SweepMode,
    /// Confirmed balance (satoshis) that triggers a sweep.
    pub threshold_sat: u64,
    /// Highest fee rate a sweep may pay, in sat/vB.
    pub max_fee_rate_sat_vb: u32,
    /// Smallest acceptable CoinJoin anonymity set.
    pub min_anonymity_set: u32,
}

impl Default for Sweep {
    fn default() -> Self {
        Sweep {
            enabled: false,
            mode: SweepMode::Payjoin,
            threshold_sat: 1_000_000,
            max_fee_rate_sat_vb: 20,
            min_anonymity_set: 5,
        }
    }
}

/// Every setting that can change while the service runs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Settings {
    pub name: String,
    pub relays: Vec<String>,
    pub public_invoicing: bool,
    pub pow_bits: u8,
    pub pow_window_secs: u64,
    pub min_confirmations: u32,
    pub default_expiry_secs: u64,
    pub max_expiry_secs: u64,
    pub min_amount_sat: u64,
    pub max_amount_sat: u64,
    pub max_open_invoices: u64,
    pub max_invoices_per_day: u64,
    pub rate_global_per_min: u32,
    pub rate_per_key_per_min: u32,
    pub rate_requests_per_min: u32,
    pub rate_requests_per_key_per_min: u32,
    pub poll_secs: u64,
    pub late_grace_secs: u64,
    /// Proxy for relay connections only; empty for a direct connection.
    pub proxy: String,
    pub sweep: Sweep,
}

/// Partial update of [`Sweep`].
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SweepPatch {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode: Option<SweepMode>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub threshold_sat: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_fee_rate_sat_vb: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_anonymity_set: Option<u32>,
}

/// Partial update of [`Settings`]; also the stored form of the overrides.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SettingsPatch {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub relays: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub public_invoicing: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pow_bits: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pow_window_secs: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_confirmations: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_expiry_secs: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_expiry_secs: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_amount_sat: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_amount_sat: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_open_invoices: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_invoices_per_day: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rate_global_per_min: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rate_per_key_per_min: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rate_requests_per_min: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rate_requests_per_key_per_min: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub poll_secs: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub late_grace_secs: Option<u64>,
    /// `socks5://`, `https://` or `http://` url; the empty string turns the proxy off.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub proxy: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sweep: Option<SweepPatch>,
}

fn take<T: Clone>(slot: &mut T, v: &Option<T>) {
    if let Some(v) = v {
        *slot = v.clone();
    }
}

fn keep<T: Clone>(slot: &mut Option<T>, v: &Option<T>) {
    if v.is_some() {
        slot.clone_from(v);
    }
}

impl SweepPatch {
    fn apply(&self, s: &mut Sweep) {
        take(&mut s.enabled, &self.enabled);
        take(&mut s.mode, &self.mode);
        take(&mut s.threshold_sat, &self.threshold_sat);
        take(&mut s.max_fee_rate_sat_vb, &self.max_fee_rate_sat_vb);
        take(&mut s.min_anonymity_set, &self.min_anonymity_set);
    }

    fn merge(&mut self, o: &SweepPatch) {
        keep(&mut self.enabled, &o.enabled);
        keep(&mut self.mode, &o.mode);
        keep(&mut self.threshold_sat, &o.threshold_sat);
        keep(&mut self.max_fee_rate_sat_vb, &o.max_fee_rate_sat_vb);
        keep(&mut self.min_anonymity_set, &o.min_anonymity_set);
    }

    fn names(&self, out: &mut Vec<String>) {
        let fields = [
            ("enabled", self.enabled.is_some()),
            ("mode", self.mode.is_some()),
            ("threshold_sat", self.threshold_sat.is_some()),
            ("max_fee_rate_sat_vb", self.max_fee_rate_sat_vb.is_some()),
            ("min_anonymity_set", self.min_anonymity_set.is_some()),
        ];
        out.extend(fields.iter().filter(|f| f.1).map(|f| format!("sweep.{}", f.0)));
    }
}

impl SettingsPatch {
    /// `base` with this patch applied on top.
    pub fn apply(&self, base: &Settings) -> Settings {
        let mut s = base.clone();
        take(&mut s.name, &self.name);
        take(&mut s.relays, &self.relays);
        take(&mut s.public_invoicing, &self.public_invoicing);
        take(&mut s.pow_bits, &self.pow_bits);
        take(&mut s.pow_window_secs, &self.pow_window_secs);
        take(&mut s.min_confirmations, &self.min_confirmations);
        take(&mut s.default_expiry_secs, &self.default_expiry_secs);
        take(&mut s.max_expiry_secs, &self.max_expiry_secs);
        take(&mut s.min_amount_sat, &self.min_amount_sat);
        take(&mut s.max_amount_sat, &self.max_amount_sat);
        take(&mut s.max_open_invoices, &self.max_open_invoices);
        take(&mut s.max_invoices_per_day, &self.max_invoices_per_day);
        take(&mut s.rate_global_per_min, &self.rate_global_per_min);
        take(&mut s.rate_per_key_per_min, &self.rate_per_key_per_min);
        take(&mut s.rate_requests_per_min, &self.rate_requests_per_min);
        take(
            &mut s.rate_requests_per_key_per_min,
            &self.rate_requests_per_key_per_min,
        );
        take(&mut s.poll_secs, &self.poll_secs);
        take(&mut s.late_grace_secs, &self.late_grace_secs);
        take(&mut s.proxy, &self.proxy);
        if let Some(sw) = &self.sweep {
            sw.apply(&mut s.sweep);
        }
        s
    }

    /// Fold a newer patch into this one; fields it sets win.
    pub fn merge(&mut self, o: &SettingsPatch) {
        keep(&mut self.name, &o.name);
        keep(&mut self.relays, &o.relays);
        keep(&mut self.public_invoicing, &o.public_invoicing);
        keep(&mut self.pow_bits, &o.pow_bits);
        keep(&mut self.pow_window_secs, &o.pow_window_secs);
        keep(&mut self.min_confirmations, &o.min_confirmations);
        keep(&mut self.default_expiry_secs, &o.default_expiry_secs);
        keep(&mut self.max_expiry_secs, &o.max_expiry_secs);
        keep(&mut self.min_amount_sat, &o.min_amount_sat);
        keep(&mut self.max_amount_sat, &o.max_amount_sat);
        keep(&mut self.max_open_invoices, &o.max_open_invoices);
        keep(&mut self.max_invoices_per_day, &o.max_invoices_per_day);
        keep(&mut self.rate_global_per_min, &o.rate_global_per_min);
        keep(&mut self.rate_per_key_per_min, &o.rate_per_key_per_min);
        keep(&mut self.rate_requests_per_min, &o.rate_requests_per_min);
        keep(
            &mut self.rate_requests_per_key_per_min,
            &o.rate_requests_per_key_per_min,
        );
        keep(&mut self.poll_secs, &o.poll_secs);
        keep(&mut self.late_grace_secs, &o.late_grace_secs);
        keep(&mut self.proxy, &o.proxy);
        if let Some(sw) = &o.sweep {
            self.sweep.get_or_insert_with(SweepPatch::default).merge(sw);
        }
    }

    /// Names of the settings this patch sets, for showing what is overridden.
    pub fn names(&self) -> Vec<String> {
        let fields = [
            ("name", self.name.is_some()),
            ("relays", self.relays.is_some()),
            ("public_invoicing", self.public_invoicing.is_some()),
            ("pow_bits", self.pow_bits.is_some()),
            ("pow_window_secs", self.pow_window_secs.is_some()),
            ("min_confirmations", self.min_confirmations.is_some()),
            ("default_expiry_secs", self.default_expiry_secs.is_some()),
            ("max_expiry_secs", self.max_expiry_secs.is_some()),
            ("min_amount_sat", self.min_amount_sat.is_some()),
            ("max_amount_sat", self.max_amount_sat.is_some()),
            ("max_open_invoices", self.max_open_invoices.is_some()),
            ("max_invoices_per_day", self.max_invoices_per_day.is_some()),
            ("rate_global_per_min", self.rate_global_per_min.is_some()),
            ("rate_per_key_per_min", self.rate_per_key_per_min.is_some()),
            ("rate_requests_per_min", self.rate_requests_per_min.is_some()),
            (
                "rate_requests_per_key_per_min",
                self.rate_requests_per_key_per_min.is_some(),
            ),
            ("poll_secs", self.poll_secs.is_some()),
            ("late_grace_secs", self.late_grace_secs.is_some()),
            ("proxy", self.proxy.is_some()),
        ];
        let mut out: Vec<String> = fields.iter().filter(|f| f.1).map(|f| f.0.to_string()).collect();
        if let Some(sw) = &self.sweep {
            sw.names(&mut out);
        }
        out
    }
}

fn within(name: &str, v: u64, (min, max): (u64, u64)) -> Result<()> {
    if v < min || v > max {
        return Err(Error::InvalidParams(format!("{name} must be between {min} and {max}")));
    }
    Ok(())
}

/// Validate a relay list for runtime use. Stricter than the environment
/// loader: on mainnet only TLS relays, unless the relay is on this machine.
pub fn check_relays(relays: &[String], network: Network) -> Result<()> {
    let (min, max) = bounds::RELAYS;
    if relays.len() < min || relays.len() > max {
        return Err(Error::InvalidParams(format!(
            "between {min} and {max} relays are required"
        )));
    }
    for (i, r) in relays.iter().enumerate() {
        if r.len() > bounds::RELAY_URL_LEN || r.chars().any(|c| c.is_whitespace() || c.is_control()) {
            return Err(Error::InvalidParams(
                "relay urls must be short and contain no spaces".into(),
            ));
        }
        if !(r.starts_with("wss://") || r.starts_with("ws://")) {
            return Err(Error::InvalidParams(format!(
                "relay '{r}' must be a ws:// or wss:// url"
            )));
        }
        // Decide on the host the connection will actually use. The raw
        // authority may carry userinfo, and "ws://127.0.0.1:80@remote" would
        // look local to a string check while connecting to "remote".
        let host = match nostr_sdk::Url::parse(r) {
            Ok(url) if nostr_sdk::RelayUrl::parse(r).is_ok() => {
                if !url.username().is_empty() || url.password().is_some() {
                    return Err(Error::InvalidParams(format!(
                        "relay '{r}' must not contain a user name or password"
                    )));
                }
                url.host_str().unwrap_or_default().to_string()
            }
            _ => String::new(),
        };
        if host.is_empty() {
            return Err(Error::InvalidParams(format!("relay '{r}' is not a valid url")));
        }
        if network == Network::Bitcoin && r.starts_with("ws://") && !is_loopback_host(&host) {
            return Err(Error::InvalidParams(format!("relay '{r}' must use wss:// on mainnet")));
        }
        if relays[..i].contains(r) {
            return Err(Error::InvalidParams(format!("relay '{r}' is listed twice")));
        }
    }
    Ok(())
}

impl Settings {
    /// Check every value and every relationship between values, except the
    /// relay list (see [`check_relays`], applied to lists set from the
    /// dashboard; the environment's list is accepted as configured).
    /// `has_admins` is false when nobody but the public could create invoices.
    pub fn validate(&self, has_admins: bool) -> Result<()> {
        use bounds::*;
        let name = self.name.trim();
        if name.is_empty() || name.chars().count() > NAME_LEN || !name.chars().all(crate::service::is_plain_text) {
            return Err(Error::InvalidParams(format!(
                "name must be 1 to {NAME_LEN} visible characters"
            )));
        }
        crate::relay::parse_proxy(&self.proxy)?;
        within("pow_bits", u64::from(self.pow_bits), POW_BITS)?;
        within("pow_window_secs", self.pow_window_secs, POW_WINDOW_SECS)?;
        within(
            "min_confirmations",
            u64::from(self.min_confirmations),
            MIN_CONFIRMATIONS,
        )?;
        within("default_expiry_secs", self.default_expiry_secs, EXPIRY_SECS)?;
        within("max_expiry_secs", self.max_expiry_secs, EXPIRY_SECS)?;
        within("min_amount_sat", self.min_amount_sat, AMOUNT_SAT)?;
        within("max_amount_sat", self.max_amount_sat, AMOUNT_SAT)?;
        within("max_open_invoices", self.max_open_invoices, MAX_OPEN_INVOICES)?;
        within("max_invoices_per_day", self.max_invoices_per_day, MAX_INVOICES_PER_DAY)?;
        within("rate_global_per_min", u64::from(self.rate_global_per_min), RATE_CREATE)?;
        within(
            "rate_per_key_per_min",
            u64::from(self.rate_per_key_per_min),
            RATE_CREATE,
        )?;
        within(
            "rate_requests_per_min",
            u64::from(self.rate_requests_per_min),
            RATE_REQUESTS,
        )?;
        within(
            "rate_requests_per_key_per_min",
            u64::from(self.rate_requests_per_key_per_min),
            RATE_REQUESTS,
        )?;
        within("poll_secs", self.poll_secs, POLL_SECS)?;
        within("late_grace_secs", self.late_grace_secs, LATE_GRACE_SECS)?;
        within("sweep.threshold_sat", self.sweep.threshold_sat, SWEEP_THRESHOLD_SAT)?;
        within(
            "sweep.max_fee_rate_sat_vb",
            u64::from(self.sweep.max_fee_rate_sat_vb),
            SWEEP_FEE_RATE,
        )?;
        within(
            "sweep.min_anonymity_set",
            u64::from(self.sweep.min_anonymity_set),
            SWEEP_ANONYMITY_SET,
        )?;
        if self.min_amount_sat > self.max_amount_sat {
            return Err(Error::InvalidParams("min_amount_sat exceeds max_amount_sat".into()));
        }
        if self.default_expiry_secs > self.max_expiry_secs {
            return Err(Error::InvalidParams(
                "default_expiry_secs exceeds max_expiry_secs".into(),
            ));
        }
        if !has_admins && !self.public_invoicing {
            return Err(Error::InvalidParams(
                "public invoicing cannot be turned off when no operators are configured".into(),
            ));
        }
        Ok(())
    }

    /// Settings that are allowed but that an operator should think twice about.
    pub fn warnings(&self, network: Network, electrum: &str, security: &crate::electrum::Security) -> Vec<String> {
        use crate::electrum::Security;
        let mut w = Vec::new();
        if !is_loopback(electrum) {
            w.push(match security {
                Security::Plain => {
                    "DEADHOP_ELECTRUM is not on this machine: the link is unencrypted and whoever runs or can \
                 intercept that server decides which invoices look paid; use tls:// with a pinned certificate"
                        .to_string()
                }
                _ => "DEADHOP_ELECTRUM is not on this machine: the link is encrypted, but whoever runs that server \
                 decides which invoices look paid"
                    .to_string(),
            });
        }
        if *security == Security::Tls {
            w.push(
                "the Electrum certificate is checked only against public authorities; pin it with \
                 DEADHOP_ELECTRUM_CERT_SHA256 (see `deadhop fingerprint`)"
                    .to_string(),
            );
        }
        if self.public_invoicing && self.pow_bits < 16 {
            w.push(format!(
                "public invoicing with only {} bits of proof of work: anyone can create invoices almost for free",
                self.pow_bits
            ));
        }
        if network == Network::Bitcoin && self.relays.iter().any(|r| r.starts_with("ws://")) {
            w.push("a relay uses ws:// (no TLS) on mainnet".to_string());
        }
        if network == Network::Bitcoin && self.min_confirmations < 2 && self.max_amount_sat > 10_000_000 {
            w.push("invoices above 0.1 BTC are marked paid after a single confirmation".to_string());
        }
        w
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::config::tests::{base, load};

    pub fn defaults() -> Settings {
        load(&base()).unwrap().settings()
    }

    fn err(s: &Settings) -> String {
        s.validate(true).unwrap_err().to_string()
    }

    #[test]
    fn defaults_are_valid_and_reflect_the_environment() {
        let s = defaults();
        s.validate(true).unwrap();
        assert_eq!(s.relays, vec!["ws://127.0.0.1:7777", "wss://relay.example"]);
        assert_eq!(s.pow_bits, 20);
        assert_eq!(s.sweep, Sweep::default());
    }

    #[test]
    fn patches_apply_merge_and_name_their_fields() {
        let base = defaults();
        let empty = SettingsPatch::default();
        assert_eq!(empty.apply(&base), base);
        assert!(empty.names().is_empty());

        let mut stored: SettingsPatch = serde_json::from_value(serde_json::json!({
            "pow_bits": 22, "relays": ["wss://a.example"], "sweep": { "enabled": true }
        }))
        .unwrap();
        let newer: SettingsPatch = serde_json::from_value(serde_json::json!({
            "name": "shop", "public_invoicing": true, "pow_window_secs": 60, "min_confirmations": 3,
            "default_expiry_secs": 600, "max_expiry_secs": 7200, "min_amount_sat": 2000,
            "max_amount_sat": 3000, "max_open_invoices": 9, "max_invoices_per_day": 99,
            "rate_global_per_min": 5, "rate_per_key_per_min": 2, "rate_requests_per_min": 50,
            "rate_requests_per_key_per_min": 10, "poll_secs": 30, "late_grace_secs": 0,
            "proxy": "socks5://127.0.0.1:9050",
            "sweep": { "mode": "coinjoin", "threshold_sat": 50000, "max_fee_rate_sat_vb": 3, "min_anonymity_set": 9 }
        }))
        .unwrap();
        stored.merge(&newer);
        let s = stored.apply(&base);
        assert_eq!((s.name.as_str(), s.pow_bits, s.relays.len()), ("shop", 22, 1));
        assert_eq!(
            (s.min_confirmations, s.poll_secs, s.late_grace_secs, s.max_open_invoices),
            (3, 30, 0, 9)
        );
        assert_eq!(
            s.sweep,
            Sweep {
                enabled: true,
                mode: SweepMode::Coinjoin,
                threshold_sat: 50_000,
                max_fee_rate_sat_vb: 3,
                min_anonymity_set: 9
            }
        );
        s.validate(true).unwrap();
        assert_eq!(stored.names().len(), 24);
        assert_eq!(s.proxy, "socks5://127.0.0.1:9050");
        assert!(stored.names().contains(&"sweep.mode".to_string()));

        // A sweep patch merged into overrides that had none.
        let mut fresh = SettingsPatch::default();
        fresh.merge(&SettingsPatch {
            sweep: Some(SweepPatch {
                enabled: Some(true),
                ..SweepPatch::default()
            }),
            ..SettingsPatch::default()
        });
        assert_eq!(fresh.names(), vec!["sweep.enabled"]);

        // Unknown fields are refused, including inside the sweep policy.
        assert!(serde_json::from_value::<SettingsPatch>(serde_json::json!({ "xpub": "x" })).is_err());
        assert!(serde_json::from_value::<SettingsPatch>(serde_json::json!({ "sweep": { "to": "x" } })).is_err());
    }

    #[test]
    fn every_value_is_bounded() {
        let ok = defaults();
        type Change = Box<dyn Fn(&mut Settings)>;
        let cases: Vec<(Change, &str)> = vec![
            (Box::new(|s| s.name = " ".into()), "name must be"),
            (Box::new(|s| s.name = "a\u{202E}b".into()), "name must be"),
            (Box::new(|s| s.name = "x".repeat(61)), "name must be"),
            (Box::new(|s| s.pow_bits = 65), "pow_bits"),
            (Box::new(|s| s.pow_window_secs = 1), "pow_window_secs"),
            (Box::new(|s| s.min_confirmations = 0), "min_confirmations"),
            (Box::new(|s| s.default_expiry_secs = 10), "default_expiry_secs"),
            (Box::new(|s| s.max_expiry_secs = 10), "max_expiry_secs"),
            (Box::new(|s| s.min_amount_sat = 0), "min_amount_sat"),
            (Box::new(|s| s.max_amount_sat = MAX_MONEY_SAT + 1), "max_amount_sat"),
            (Box::new(|s| s.max_open_invoices = 0), "max_open_invoices"),
            (Box::new(|s| s.max_invoices_per_day = 0), "max_invoices_per_day"),
            (Box::new(|s| s.rate_global_per_min = 0), "rate_global_per_min"),
            (Box::new(|s| s.rate_per_key_per_min = 0), "rate_per_key_per_min"),
            (Box::new(|s| s.rate_requests_per_min = 0), "rate_requests_per_min"),
            (
                Box::new(|s| s.rate_requests_per_key_per_min = 0),
                "rate_requests_per_key_per_min",
            ),
            (Box::new(|s| s.poll_secs = 0), "poll_secs"),
            (Box::new(|s| s.late_grace_secs = 31 * 86_400), "late_grace_secs"),
            (Box::new(|s| s.proxy = "ftp://x:1".into()), "proxy: scheme"),
            (Box::new(|s| s.sweep.threshold_sat = 1), "sweep.threshold_sat"),
            (
                Box::new(|s| s.sweep.max_fee_rate_sat_vb = 0),
                "sweep.max_fee_rate_sat_vb",
            ),
            (Box::new(|s| s.sweep.min_anonymity_set = 1), "sweep.min_anonymity_set"),
            (
                Box::new(|s| s.min_amount_sat = s.max_amount_sat + 1),
                "min_amount_sat exceeds",
            ),
            (
                Box::new(|s| s.default_expiry_secs = s.max_expiry_secs + 1),
                "default_expiry_secs exceeds",
            ),
        ];
        for (change, expect) in cases {
            let mut s = ok.clone();
            change(&mut s);
            assert!(err(&s).contains(expect), "{expect}: {}", err(&s));
        }
        let mut closed = ok.clone();
        closed.public_invoicing = false;
        assert!(closed.validate(false).unwrap_err().to_string().contains("no operators"));
    }

    #[test]
    fn relay_lists_are_checked() {
        let r = |v: &[&str], n: Network| check_relays(&v.iter().map(|s| s.to_string()).collect::<Vec<_>>(), n);
        assert!(r(&["wss://relay.example", "ws://127.0.0.1:7777"], Network::Bitcoin).is_ok());
        assert!(r(&["ws://relay.example"], Network::Regtest).is_ok());
        let bad: Vec<(Vec<&str>, &str)> = vec![
            (vec![], "between 1 and 8"),
            (vec!["wss://a.example"; 9], "between 1 and 8"),
            (vec!["https://a.example"], "must be a ws://"),
            (vec!["wss://"], "not a valid url"),
            (vec!["wss://a b.example"], "no spaces"),
            (vec!["wss://a.example", "wss://a.example"], "listed twice"),
        ];
        for (list, expect) in bad {
            assert!(
                r(&list, Network::Regtest).unwrap_err().to_string().contains(expect),
                "{expect}"
            );
        }
        let long = format!("wss://{}.example", "a".repeat(200));
        assert!(r(&[&long], Network::Regtest).unwrap_err().to_string().contains("short"));
        assert!(r(&["ws://relay.example"], Network::Bitcoin)
            .unwrap_err()
            .to_string()
            .contains("wss:// on mainnet"));
        // Local plaintext relays stay allowed on mainnet, by any loopback spelling.
        assert!(r(&["ws://localhost:7777", "ws://[::1]:7777/path"], Network::Bitcoin).is_ok());
        // Userinfo must not make a remote host pass for a local one (issue #7):
        // the connection would go to evil.example over plaintext.
        for spoof in [
            "ws://127.0.0.1:80@evil.example",
            "ws://localhost:9050@evil.example",
            "ws://[::1]:1@evil.example",
            "wss://user@relay.example",
            "wss://:secret@relay.example",
        ] {
            for net in [Network::Bitcoin, Network::Regtest] {
                assert!(
                    r(&[spoof], net)
                        .unwrap_err()
                        .to_string()
                        .contains("user name or password"),
                    "{spoof}"
                );
            }
        }
    }

    #[test]
    fn warnings_name_risky_choices() {
        let mut s = defaults();
        let plain = crate::electrum::Security::Plain;
        assert!(s.warnings(Network::Regtest, "127.0.0.1:50001", &plain).is_empty());
        s.public_invoicing = true;
        s.pow_bits = 8;
        s.relays = vec!["ws://relay.example".into()];
        s.max_amount_sat = 50_000_000;
        let w = s.warnings(Network::Bitcoin, "10.0.0.2:50001", &plain);
        assert_eq!(w.len(), 4, "{w:?}");
        assert!(w[0].contains("unencrypted"));
        let tls = s.warnings(Network::Regtest, "10.0.0.2:50002", &crate::electrum::Security::Tls);
        assert!(
            tls[0].contains("link is encrypted") && tls[1].contains("pin it"),
            "{tls:?}"
        );
        let pinned = crate::electrum::Security::Pinned(vec![[0; 32]]);
        assert_eq!(
            s.warnings(Network::Regtest, "127.0.0.1:50002", &pinned).len(),
            1,
            "only the weak work warning"
        );
    }
}
