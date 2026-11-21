//! Settings an operator may change at runtime from the dashboard.
//!
//! The environment supplies the defaults; changes made remotely are stored as
//! overrides in the database and win over the environment until reset.
//! Anything that decides where money goes or whom the service trusts (the
//! xpub, network, Electrum server, operator list, data directory) is
//! deliberately not here: a stolen operator key must not be able to redirect
//! payments or lock the owner out.

use std::collections::BTreeMap;

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
    /// Most proxies in the list.
    pub const PROXIES: usize = 8;
    /// Every switch of proxy reconnects every relay, so intervals have a floor.
    pub const PROXY_INTERVAL_SECS: (u64, u64) = (60, 7 * 86_400);
    pub const PROXY_ID_LEN: usize = 24;
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

// One proxy in the ordered list. `id` is stable across reorders and edits, so
// a password the dashboard never sees (it shows `***`) is kept when the list
// is saved again.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ProxyEntry {
    pub id: String,
    // Optional label for the dashboard.
    pub name: String,
    // `socks5://`, `https://` or `http://` url.
    pub url: String,
    // The keys (by id) whose relay connections may use this proxy; empty is every key.
    #[serde(default)]
    pub keys: Vec<String>,
}

// How the proxies in the list are used.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ProxyMode {
    // The first reachable proxy in list order; the next one when it fails.
    Failover,
    // Take the reachable proxies in turn, one change every `rotate_secs`.
    RoundRobin,
    // Change to a random reachable proxy after a random time between
    // `chaos_min_secs` and `chaos_max_secs`.
    Chaos,
}

// What the service does while no proxy is reachable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum OfflinePolicy {
    // Disconnect from every relay: the service is off until a proxy is back.
    KillSwitch,
    // Connect straight (clearnet) but answer operators only, so the proxies
    // can be repaired (unless they are locked on the host).
    AdminOnly,
    // Connect straight but answer invoicing only; operators are not served.
    InvoicingOnly,
    // Connect straight and carry on as usual.
    Open,
}

// Relay proxies: an ordered list, how to choose among them, and what to do
// when none works.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ProxySettings {
    pub list: Vec<ProxyEntry>,
    pub mode: ProxyMode,
    // Round robin: seconds between changes.
    pub rotate_secs: u64,
    // Chaos: shortest and longest time (seconds) before the next change.
    pub chaos_min_secs: u64,
    pub chaos_max_secs: u64,
    pub offline: OfflinePolicy,
    // The operator runs a transparent proxy around this service: it is not
    // checked, the list is ignored and privacy warnings about the proxy are
    // not raised.
    pub transparent: bool,
}

impl Default for ProxySettings {
    fn default() -> Self {
        ProxySettings {
            list: Vec::new(),
            mode: ProxyMode::Failover,
            rotate_secs: 3_600,
            chaos_min_secs: 300,
            chaos_max_secs: 900,
            offline: OfflinePolicy::KillSwitch,
            transparent: false,
        }
    }
}

// A proxy as sent from the dashboard. A missing `id` makes a new entry; a
// url shown as `***` (what the dashboard receives) keeps the stored password.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ProxyEntryInput {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub url: String,
    // The keys that may use this proxy; left out or empty is every key.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub keys: Option<Vec<String>>,
}

// Partial update of [`ProxySettings`]; `list` replaces the whole list.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ProxyPatch {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub list: Option<Vec<ProxyEntryInput>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode: Option<ProxyMode>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rotate_secs: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chaos_min_secs: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chaos_max_secs: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub offline: Option<OfflinePolicy>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transparent: Option<bool>,
}

fn new_proxy_id() -> String {
    let mut b = [0u8; 4];
    getrandom::getrandom(&mut b).expect("the system has a random source");
    format!("p{}", hex::encode(b))
}

fn entries_from(list: &[ProxyEntryInput]) -> Vec<ProxyEntry> {
    list.iter()
        .enumerate()
        .map(|(i, e)| ProxyEntry {
            id: e.id.clone().unwrap_or_else(|| format!("p{i}")),
            name: e.name.clone().unwrap_or_default(),
            url: e.url.clone(),
            keys: e.keys.clone().unwrap_or_default(),
        })
        .collect()
}

impl ProxyPatch {
    // A patch that sets the whole list and nothing else.
    pub fn with_list(list: &[ProxyEntry]) -> ProxyPatch {
        ProxyPatch {
            list: Some(
                list.iter()
                    .map(|e| ProxyEntryInput {
                        id: Some(e.id.clone()),
                        name: Some(e.name.clone()),
                        url: e.url.clone(),
                        keys: Some(e.keys.clone()),
                    })
                    .collect(),
            ),
            ..ProxyPatch::default()
        }
    }

    // Give new entries an id, and put back the real url of entries sent with
    // a hidden password. Afterwards every entry has an id and a usable url.
    pub fn resolve(&mut self, current: &[ProxyEntry]) -> Result<()> {
        let Some(list) = &mut self.list else { return Ok(()) };
        for input in list.iter_mut() {
            let stored = match &input.id {
                Some(id) => current.iter().find(|c| &c.id == id),
                None => current.iter().find(|c| crate::relay::redact(&c.url) == input.url),
            };
            if input.url.contains("***") {
                match stored.filter(|c| crate::relay::redact(&c.url) == input.url) {
                    Some(c) => input.url.clone_from(&c.url),
                    None => {
                        return Err(Error::InvalidParams(
                            "proxy: the password is hidden; type the full address again".into(),
                        ))
                    }
                }
            }
            if input.id.is_none() {
                input.id = Some(stored.map_or_else(new_proxy_id, |c| c.id.clone()));
            }
        }
        Ok(())
    }

    fn apply(&self, s: &mut ProxySettings) {
        if let Some(list) = &self.list {
            s.list = entries_from(list);
        }
        take(&mut s.mode, &self.mode);
        take(&mut s.rotate_secs, &self.rotate_secs);
        take(&mut s.chaos_min_secs, &self.chaos_min_secs);
        take(&mut s.chaos_max_secs, &self.chaos_max_secs);
        take(&mut s.offline, &self.offline);
        take(&mut s.transparent, &self.transparent);
    }

    fn merge(&mut self, o: &ProxyPatch) {
        keep(&mut self.list, &o.list);
        keep(&mut self.mode, &o.mode);
        keep(&mut self.rotate_secs, &o.rotate_secs);
        keep(&mut self.chaos_min_secs, &o.chaos_min_secs);
        keep(&mut self.chaos_max_secs, &o.chaos_max_secs);
        keep(&mut self.offline, &o.offline);
        keep(&mut self.transparent, &o.transparent);
    }

    fn names(&self, out: &mut Vec<String>) {
        let fields = [
            ("list", self.list.is_some()),
            ("mode", self.mode.is_some()),
            ("rotate_secs", self.rotate_secs.is_some()),
            ("chaos_min_secs", self.chaos_min_secs.is_some()),
            ("chaos_max_secs", self.chaos_max_secs.is_some()),
            ("offline", self.offline.is_some()),
            ("transparent", self.transparent.is_some()),
        ];
        out.extend(fields.iter().filter(|f| f.1).map(|f| format!("proxies.{}", f.0)));
    }
}

impl ProxySettings {
    pub fn validate(&self) -> Result<()> {
        use bounds::*;
        if self.list.len() > PROXIES {
            return Err(Error::InvalidParams(format!("at most {PROXIES} proxies")));
        }
        for (i, e) in self.list.iter().enumerate() {
            let ok_id = !e.id.is_empty()
                && e.id.len() <= PROXY_ID_LEN
                && e.id
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_');
            if !ok_id {
                return Err(Error::InvalidParams("proxy: bad id".into()));
            }
            if e.name.chars().count() > NAME_LEN || !e.name.chars().all(crate::service::is_plain_text) {
                return Err(Error::InvalidParams(format!(
                    "proxy name must be at most {NAME_LEN} visible characters"
                )));
            }
            crate::keys::check_ids(&e.keys, "proxy")?;
            if crate::relay::parse_proxy(&e.url)?.is_none() {
                return Err(Error::InvalidParams("proxy: the address is empty".into()));
            }
            if self.list[..i].iter().any(|o| o.id == e.id) {
                return Err(Error::InvalidParams("proxy: the same id is used twice".into()));
            }
            if self.list[..i].iter().any(|o| o.url == e.url) {
                return Err(Error::InvalidParams("proxy: the same address is listed twice".into()));
            }
        }
        // A key with no proxy would connect straight while the others hide
        // behind one. That must be chosen by taking the proxies away, not by
        // forgetting a key.
        if !self.transparent && !self.list.is_empty() {
            if let Some(k) = crate::keys::KEYS
                .iter()
                .find(|k| !self.list.iter().any(|e| crate::keys::assigned(&e.keys, k.id)))
            {
                return Err(Error::InvalidParams(format!(
                    "no proxy is assigned to the {} key, which would connect without one",
                    k.label.to_lowercase()
                )));
            }
        }
        within("proxies.rotate_secs", self.rotate_secs, PROXY_INTERVAL_SECS)?;
        within("proxies.chaos_min_secs", self.chaos_min_secs, PROXY_INTERVAL_SECS)?;
        within("proxies.chaos_max_secs", self.chaos_max_secs, PROXY_INTERVAL_SECS)?;
        if self.chaos_min_secs > self.chaos_max_secs {
            return Err(Error::InvalidParams(
                "proxies.chaos_min_secs exceeds proxies.chaos_max_secs".into(),
            ));
        }
        Ok(())
    }

    // Replace each url's password with `***`.
    pub fn redacted(&self) -> ProxySettings {
        let mut s = self.clone();
        s.list.iter_mut().for_each(|e| e.url = crate::relay::redact(&e.url));
        s
    }
}

/// Every setting that can change while the service runs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Settings {
    pub name: String,
    pub relays: Vec<String>,
    /// Relays that only some keys use: relay url to key ids. A relay not
    /// listed here is used by every key.
    #[serde(default)]
    pub relay_keys: BTreeMap<String, Vec<String>>,
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
    /// Proxies for relay connections only.
    pub proxies: ProxySettings,
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
    /// Replaces the whole map of relays used by only some keys.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub relay_keys: Option<BTreeMap<String, Vec<String>>>,
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
    /// Old form of a single proxy url (empty turns it off); read for
    /// settings saved before there was a list, and turned into one entry.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub proxy: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub proxies: Option<ProxyPatch>,
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
        take(&mut s.relay_keys, &self.relay_keys);
        // Entries for relays that are gone, or that name no key, mean nothing.
        let relays = s.relays.clone();
        s.relay_keys
            .retain(|url, keys| !keys.is_empty() && relays.iter().any(|r| same_relay(r, url)));
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
        if let Some(old) = &self.proxy {
            s.proxies.list = if old.is_empty() {
                Vec::new()
            } else {
                vec![ProxyEntry {
                    id: "saved".into(),
                    name: String::new(),
                    url: old.clone(),
                    keys: Vec::new(),
                }]
            };
        }
        if let Some(p) = &self.proxies {
            p.apply(&mut s.proxies);
        }
        if let Some(sw) = &self.sweep {
            sw.apply(&mut s.sweep);
        }
        s
    }

    /// Fold a newer patch into this one; fields it sets win.
    pub fn merge(&mut self, o: &SettingsPatch) {
        keep(&mut self.name, &o.name);
        keep(&mut self.relays, &o.relays);
        keep(&mut self.relay_keys, &o.relay_keys);
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
        if let Some(p) = &o.proxies {
            if o.proxy.is_none() && p.list.is_some() {
                // A new list replaces the old single address.
                self.proxy = None;
            }
            self.proxies.get_or_insert_with(ProxyPatch::default).merge(p);
        }
        if let Some(sw) = &o.sweep {
            self.sweep.get_or_insert_with(SweepPatch::default).merge(sw);
        }
    }

    /// Names of the settings this patch sets, for showing what is overridden.
    pub fn names(&self) -> Vec<String> {
        let fields = [
            ("name", self.name.is_some()),
            ("relays", self.relays.is_some()),
            ("relay_keys", self.relay_keys.is_some()),
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
        if let Some(p) = &self.proxies {
            p.names(&mut out);
        }
        if let Some(sw) = &self.sweep {
            sw.names(&mut out);
        }
        out
    }
}

/// Whether two relay addresses are the same relay.
pub fn same_relay(a: &str, b: &str) -> bool {
    crate::manage::relay_key(a) == crate::manage::relay_key(b)
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
    /// The keys (by id) that use this relay.
    pub fn relay_users(&self, relay: &str) -> Vec<String> {
        match self.relay_keys.iter().find(|(url, _)| same_relay(url, relay)) {
            Some((_, keys)) => keys.clone(),
            None => crate::keys::KEYS.iter().map(|k| k.id.to_string()).collect(),
        }
    }

    /// The relays one key connects to, in list order.
    pub fn relays_for(&self, key: &str) -> Vec<String> {
        self.relays
            .iter()
            .filter(|r| self.relay_users(r).iter().any(|k| k == key))
            .cloned()
            .collect()
    }

    /// The relays used by any of the keys a proxy carries (empty: every key),
    /// in list order.
    pub fn relays_through(&self, proxy_keys: &[String]) -> Vec<String> {
        self.relays
            .iter()
            .filter(|r| self.relay_users(r).iter().any(|k| crate::keys::assigned(proxy_keys, k)))
            .cloned()
            .collect()
    }

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
        self.proxies.validate()?;
        for keys in self.relay_keys.values() {
            crate::keys::check_ids(keys, "relay")?;
        }
        if let Some(k) = crate::keys::KEYS.iter().find(|k| self.relays_for(k.id).is_empty()) {
            return Err(Error::InvalidParams(format!(
                "no relay is assigned to the {} key, which could then not be reached",
                k.label.to_lowercase()
            )));
        }
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

    fn entry(id: &str, url: &str) -> ProxyEntry {
        ProxyEntry {
            id: id.into(),
            name: String::new(),
            url: url.into(),
            keys: Vec::new(),
        }
    }

    #[test]
    fn a_proxy_list_keeps_hidden_passwords_through_edits_and_reorders() {
        let current = vec![
            entry("one", "socks5://u:secret1@10.0.0.1:1080"),
            entry("two", "http://u:secret2@10.0.0.2:3128"),
        ];
        // What the dashboard holds (passwords hidden), reordered, plus a new one.
        let mut patch = ProxyPatch {
            list: Some(vec![
                ProxyEntryInput {
                    id: Some("two".into()),
                    name: Some("second".into()),
                    url: crate::relay::redact(&current[1].url),
                    keys: None,
                },
                ProxyEntryInput {
                    id: Some("one".into()),
                    name: None,
                    url: crate::relay::redact(&current[0].url),
                    keys: None,
                },
                ProxyEntryInput {
                    id: None,
                    name: None,
                    url: "socks5://10.0.0.3:9050".into(),
                    keys: None,
                },
            ]),
            ..ProxyPatch::default()
        };
        patch.resolve(&current).unwrap();
        let mut s = ProxySettings::default();
        patch.apply(&mut s);
        s.validate().unwrap();
        assert_eq!(
            s.list.iter().map(|e| e.url.as_str()).collect::<Vec<_>>(),
            [
                "http://u:secret2@10.0.0.2:3128",
                "socks5://u:secret1@10.0.0.1:1080",
                "socks5://10.0.0.3:9050"
            ]
        );
        assert_eq!(s.list[0].id, "two");
        assert!(s.list[2].id.starts_with('p') && s.list[2].id.len() > 3);

        // Without an id, the hidden url is matched to the entry showing the same text.
        let mut by_text = ProxyPatch::with_list(&[]);
        by_text.list = Some(vec![ProxyEntryInput {
            id: None,
            name: None,
            url: crate::relay::redact(&current[0].url),
            keys: None,
        }]);
        by_text.resolve(&current).unwrap();
        assert_eq!(by_text.list.as_ref().unwrap()[0].url, current[0].url);
        assert_eq!(by_text.list.as_ref().unwrap()[0].id.as_deref(), Some("one"));

        // A hidden password that matches nothing is refused, not stored as '***'.
        let mut bad = ProxyPatch {
            list: Some(vec![ProxyEntryInput {
                id: Some("one".into()),
                name: None,
                url: "socks5://u:***@evil.example:1".into(),
                keys: None,
            }]),
            ..ProxyPatch::default()
        };
        assert!(bad.resolve(&current).unwrap_err().to_string().contains("hidden"));
    }

    #[test]
    fn proxy_ids_and_names_are_checked_and_an_empty_old_proxy_clears_the_list() {
        let mut p = ProxySettings {
            list: vec![entry("Bad Id", "socks5://h:1")],
            ..ProxySettings::default()
        };
        assert!(p.validate().unwrap_err().to_string().contains("bad id"));
        p.list = vec![ProxyEntry {
            name: "x".repeat(61),
            ..entry("a", "socks5://h:1")
        }];
        assert!(p.validate().unwrap_err().to_string().contains("proxy name"));
        p.list = vec![ProxyEntry {
            name: "Tor".into(),
            ..entry("a", "socks5://h:1")
        }];
        p.validate().unwrap();
        // Entries stored without an id (never written by this version) still get one.
        let bare: ProxyPatch =
            serde_json::from_value(serde_json::json!({ "list": [{ "url": "socks5://h:1" }] })).unwrap();
        let mut applied = ProxySettings::default();
        bare.apply(&mut applied);
        assert_eq!(applied.list[0].id, "p0");
        let mut s = defaults();
        s.proxies = p;
        let cleared: SettingsPatch = serde_json::from_value(serde_json::json!({ "proxy": "" })).unwrap();
        assert!(cleared.apply(&s).proxies.list.is_empty());
    }

    #[test]
    fn proxy_patches_merge_and_old_saved_proxies_are_replaced_by_a_list() {
        let mut stored: SettingsPatch = serde_json::from_value(serde_json::json!({ "proxy": "socks5://h:1" })).unwrap();
        let newer: SettingsPatch = serde_json::from_value(serde_json::json!({
            "proxies": { "mode": "chaos", "list": [{ "id": "a", "url": "socks5://h:2" }] }
        }))
        .unwrap();
        stored.merge(&newer);
        assert!(stored.proxy.is_none());
        let s = stored.apply(&defaults());
        assert_eq!(
            (s.proxies.mode, s.proxies.list[0].url.as_str()),
            (ProxyMode::Chaos, "socks5://h:2")
        );
        assert_eq!(stored.names(), vec!["proxies.list", "proxies.mode"]);
        // Switching a policy keeps the list.
        stored.merge(&SettingsPatch {
            proxies: Some(ProxyPatch {
                offline: Some(OfflinePolicy::Open),
                transparent: Some(true),
                ..ProxyPatch::default()
            }),
            ..SettingsPatch::default()
        });
        let s = stored.apply(&defaults());
        assert!(s.proxies.transparent && s.proxies.offline == OfflinePolicy::Open);
        assert_eq!(s.proxies.list.len(), 1);
        // Redaction for display.
        let p = ProxySettings {
            list: vec![entry("a", "socks5://u:pw@h:1")],
            ..ProxySettings::default()
        };
        assert_eq!(p.redacted().list[0].url, "socks5://u:***@h:1");
        assert!(serde_json::from_value::<SettingsPatch>(serde_json::json!({ "proxies": { "x": 1 } })).is_err());
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
        // A proxy saved before there was a list is kept, as the first entry.
        assert_eq!(s.proxies.list.len(), 1);
        assert_eq!(s.proxies.list[0].url, "socks5://127.0.0.1:9050");
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
            (
                Box::new(|s| s.proxies.list = vec![entry("a", "ftp://x:1")]),
                "proxy: scheme",
            ),
            (Box::new(|s| s.proxies.rotate_secs = 59), "proxies.rotate_secs"),
            (
                Box::new(|s| s.proxies.chaos_max_secs = 8 * 86_400),
                "proxies.chaos_max_secs",
            ),
            (
                Box::new(|s| {
                    s.proxies.chaos_min_secs = 900;
                    s.proxies.chaos_max_secs = 300;
                }),
                "exceeds",
            ),
            (
                Box::new(|s| s.proxies.list = vec![entry("a", "socks5://h:1"), entry("a", "socks5://h:2")]),
                "same id",
            ),
            (
                Box::new(|s| s.proxies.list = vec![entry("a", "socks5://h:1"), entry("b", "socks5://h:1")]),
                "listed twice",
            ),
            (Box::new(|s| s.proxies.list = vec![entry("a", "")]), "address is empty"),
            (
                Box::new(|s| {
                    s.proxies.list = (0..9)
                        .map(|i| entry(&format!("p{i}"), &format!("socks5://h:{}", i + 1)))
                        .collect()
                }),
                "at most 8",
            ),
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

    fn ids(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn relays_and_proxies_are_assigned_to_keys() {
        let mut s = defaults();
        let (a, b) = (s.relays[0].clone(), s.relays[1].clone());
        // Nothing assigned: every key uses every relay.
        assert_eq!(s.relays_for("invoices"), s.relays);
        assert_eq!(s.relays_for("admin"), s.relays);
        assert_eq!(s.relay_users(&a), ids(&["invoices", "admin"]));
        s.validate(true).unwrap();

        // One relay for the public key only (written with a trailing slash).
        s.relay_keys.insert(format!("{b}/"), ids(&["invoices"]));
        assert_eq!(s.relay_users(&b), ids(&["invoices"]));
        assert_eq!(s.relays_for("admin"), std::slice::from_ref(&a));
        assert_eq!(s.relays_for("invoices"), s.relays);
        assert_eq!(s.relays_through(&ids(&["admin"])), std::slice::from_ref(&a));
        assert_eq!(s.relays_through(&[]), s.relays);
        s.validate(true).unwrap();
        assert!(same_relay("wss://x.example/", "wss://x.example"));
    }

    #[test]
    fn a_key_must_keep_a_relay_and_a_proxy() {
        let mut s = defaults();
        let all = s.relays.clone();
        // Every relay given to the administration key leaves the public key with none.
        s.relay_keys = all.iter().map(|r| (r.clone(), ids(&["admin"]))).collect();
        let e = err(&s);
        assert!(e.contains("no relay is assigned to the public invoices key"), "{e}");
        s.relay_keys = all.iter().map(|r| (r.clone(), ids(&["invoices"]))).collect();
        let e = err(&s);
        assert!(e.contains("no relay is assigned to the administration key"), "{e}");
        s.relay_keys = [(all[0].clone(), ids(&["root"]))].into();
        let e = err(&s);
        assert!(e.contains("no key 'root'"), "{e}");
        s.relay_keys.clear();

        // The same for proxies: forgetting a key would send it out unprotected.
        s.proxies.list = vec![entry("a", "socks5://h:1"), entry("b", "socks5://h:2")];
        s.validate(true).unwrap();
        s.proxies.list.iter_mut().for_each(|e| e.keys = ids(&["admin"]));
        let e = err(&s);
        assert!(e.contains("no proxy is assigned to the public invoices key"), "{e}");
        s.proxies.list[1].keys = ids(&["invoices"]);
        s.validate(true).unwrap();
        s.proxies.list[1].keys = ids(&["invoices", "invoices"]);
        let e = err(&s);
        assert!(e.contains("named twice"), "{e}");
        s.proxies.list[1].keys = ids(&["nobody"]);
        let e = err(&s);
        assert!(e.contains("no key 'nobody'"), "{e}");
        // A transparent proxy does the work: the list is not used, so it needs no assignment.
        s.proxies.list[1].keys = ids(&["admin"]);
        s.proxies.transparent = true;
        s.validate(true).unwrap();
    }

    #[test]
    fn saved_settings_without_keys_mean_every_key() {
        let old: ProxyEntry =
            serde_json::from_value(serde_json::json!({ "id": "a", "name": "", "url": "socks5://h:1" })).unwrap();
        assert!(old.keys.is_empty());
        let input: ProxyEntryInput = serde_json::from_value(serde_json::json!({ "url": "socks5://h:1" })).unwrap();
        assert_eq!(entries_from(&[input])[0].keys, Vec::<String>::new());
        let some = ProxyEntryInput {
            keys: Some(ids(&["admin"])),
            ..serde_json::from_value(serde_json::json!({ "url": "socks5://h:1" })).unwrap()
        };
        assert_eq!(entries_from(&[some])[0].keys, ids(&["admin"]));
        // The list sent back keeps each proxy's keys.
        let kept = ProxyPatch::with_list(&[ProxyEntry {
            keys: ids(&["admin"]),
            ..entry("a", "socks5://h:1")
        }]);
        assert_eq!(kept.list.unwrap()[0].keys, Some(ids(&["admin"])));
    }

    #[test]
    fn relay_assignments_travel_in_patches() {
        let base = defaults();
        let (a, b) = (base.relays[0].clone(), base.relays[1].clone());
        let patch: SettingsPatch = serde_json::from_value(serde_json::json!({
            "relay_keys": { (b.clone()): ["invoices"], "wss://gone.example": ["admin"], (a.clone()): [] }
        }))
        .unwrap();
        assert_eq!(patch.names(), ["relay_keys"]);
        // Entries for relays that are not listed, or that name no key, mean nothing.
        let applied = patch.apply(&base);
        assert_eq!(applied.relay_keys, [(b.clone(), ids(&["invoices"]))].into());

        // Removing a relay drops its assignment with it.
        let drop_b: SettingsPatch = serde_json::from_value(serde_json::json!({ "relays": [a.clone()] })).unwrap();
        let mut stored = patch.clone();
        stored.merge(&drop_b);
        assert!(stored.apply(&base).relay_keys.is_empty());

        // A newer assignment replaces the stored one as a whole.
        let newer: SettingsPatch =
            serde_json::from_value(serde_json::json!({ "relay_keys": { (a.clone()): ["admin"] } })).unwrap();
        let mut stored = patch;
        stored.merge(&newer);
        assert_eq!(stored.apply(&base).relay_keys, [(a, ids(&["admin"]))].into());
    }
}
