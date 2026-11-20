//! Operator tools beyond invoicing: health, metrics, settings, operator keys,
//! service control, backups and wiping.
//!
//! Every tool here is operator-only. Every tool that changes state also needs
//! a fresh request event addressed to this service that was never used before
//! (checked against a list that survives restarts), and leaves an entry in
//! the audit log.

use std::fs;
use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
use std::path::PathBuf;
use std::sync::atomic::Ordering;

use rmcp::schemars::{self, JsonSchema};
use serde::{Deserialize, Serialize};

use crate::config::{is_loopback, parse_pubkey};
use crate::db::OperatorRow;
use crate::error::{Error, Result};
use crate::relay::redact;
use crate::service::{
    is_plain_text, Caller, RelayState, Service, DAY_SECS, META_LAST_BACKUP, META_PAUSED, META_SETTINGS,
};
use crate::settings::{check_relays, Settings, SettingsPatch, SweepMode};

/// Directory (inside the data directory) holding database backups.
pub const BACKUP_DIR: &str = "backups";
/// Backups kept; older ones are deleted when a new one is made.
pub const BACKUPS_KEPT: usize = 10;
/// Operator keys that may be added from the dashboard.
pub const MAX_EXTRA_OPERATORS: usize = 20;
/// Text an operator must type to wipe the database.
pub const WIPE_CONFIRMATION: &str = "WIPE";
const LABEL_LEN: usize = 60;

// ---- Wire types -----------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct ServiceHealth {
    pub version: String,
    /// When this process started (unix seconds).
    pub started_at: u64,
    pub uptime_secs: u64,
    /// When this service first ran with this database.
    pub online_since: u64,
    /// Invoicing stopped by an operator.
    pub paused: bool,
    /// In-process restarts since the process started.
    pub restarts: u32,
    pub restart_pending: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct ChainHealth {
    pub ok: bool,
    pub tip_height: Option<u32>,
    pub last_poll_at: Option<u64>,
    pub poll_secs: u64,
    pub watch_errors: u64,
    /// Whether the Electrum server is on this machine.
    pub backend_local: bool,
    /// `tcp`, `tls` (public authorities) or `tls-pinned`.
    pub transport: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct DatabaseHealth {
    /// SQLite quick integrity check passed.
    pub ok: bool,
    pub size_bytes: u64,
    pub invoices: u64,
    pub backups: u32,
    pub last_backup_at: Option<u64>,
}

/// Result of `get_health`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct Health {
    pub service: ServiceHealth,
    pub relays: Vec<RelayState>,
    /// Proxy used for relay connections (password hidden), empty if direct.
    pub proxy: String,
    pub chain: ChainHealth,
    pub database: DatabaseHealth,
    pub warnings: Vec<String>,
}

/// Payments within one trailing window.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct PaymentWindow {
    /// `24h`, `7d`, `30d` or `365d`.
    pub label: String,
    pub secs: u64,
    /// False while the service has been online for less than the window; the
    /// figures then cover only the time it has been online.
    pub available: bool,
    pub count: u64,
    pub sat: u64,
}

/// What the sweep policy would do now. Nothing is executed by this service.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct SweepStatus {
    pub enabled: bool,
    pub mode: SweepMode,
    pub threshold_sat: u64,
    /// Confirmed satoshis received (spends are not visible to a watch-only service).
    pub received_sat: u64,
    /// Enabled and at or above the threshold.
    pub due: bool,
}

/// Result of `get_metrics`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct Metrics {
    pub now: u64,
    pub online_since: u64,
    /// Addresses handed out so far (the next derivation index).
    pub addresses_created: u32,
    pub invoices: u64,
    /// Invoices paid in full and confirmed.
    pub payments: u64,
    pub received_sat: u64,
    pub windows: Vec<PaymentWindow>,
    pub sweep: SweepStatus,
}

/// Span of a payment series.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Range {
    /// 24 hourly buckets.
    Day,
    /// 7 daily buckets.
    Week,
    /// 30 daily buckets.
    Month,
    /// 52 weekly buckets.
    Year,
}

impl Range {
    fn buckets(self) -> (u64, usize) {
        match self {
            Range::Day => (3_600, 24),
            Range::Week => (DAY_SECS, 7),
            Range::Month => (DAY_SECS, 30),
            Range::Year => (7 * DAY_SECS, 52),
        }
    }
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SeriesParams {
    pub range: Range,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct SeriesPoint {
    /// Bucket start (unix seconds, UTC-aligned).
    pub start: u64,
    pub count: u64,
    pub sat: u64,
}

/// Result of `get_payment_series`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct Series {
    pub range: Range,
    pub bucket_secs: u64,
    pub online_since: u64,
    pub points: Vec<SeriesPoint>,
}

/// An operator key.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct Operator {
    /// Hex public key.
    pub pubkey: String,
    pub label: String,
    /// `environment` keys come from DEADHOP_ADMINS and cannot be removed remotely.
    pub source: String,
    pub added_at: Option<u64>,
}

/// What can only be changed on the host.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct FixedSettings {
    pub network: String,
    pub electrum: String,
    /// `tcp`, `tls` (public authorities) or `tls-pinned`.
    pub electrum_transport: String,
    /// Receive address at index 0, to compare with the wallet.
    pub first_address: String,
    pub announce: bool,
    /// DEADHOP_PROXY_LOCKED: the proxy can only be changed on the host.
    pub proxy_locked: bool,
}

/// Result of the settings tools.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct SettingsView {
    /// Live values.
    pub settings: Settings,
    /// Values from the environment, used where nothing is overridden.
    pub defaults: Settings,
    /// Names of the settings changed from the dashboard.
    pub overridden: Vec<String>,
    pub fixed: FixedSettings,
    pub operators: Vec<Operator>,
    pub restart_pending: bool,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct UpdateSettingsParams {
    /// Only the settings to change.
    pub patch: SettingsPatch,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AddOperatorParams {
    /// Hex, `npub` or `nprofile`.
    pub pubkey: String,
    #[serde(default)]
    pub label: Option<String>,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RemoveOperatorParams {
    /// Hex, `npub` or `nprofile`.
    pub pubkey: String,
}

/// What `control_service` should do.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum ControlAction {
    /// Refuse new invoices. Payments keep being watched; operators can still connect.
    Stop,
    /// Accept new invoices again.
    Start,
    /// Reconnect to the relays and reload settings, in-process.
    Restart,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ControlParams {
    pub action: ControlAction,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct ControlResult {
    pub paused: bool,
    pub restart_pending: bool,
}

/// A database backup on the service host.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct Backup {
    /// File name inside `<data dir>/backups`.
    pub file: String,
    pub size_bytes: u64,
    pub created_at: u64,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct WipeParams {
    /// Must be exactly `WIPE`.
    pub confirm: String,
    /// Also forget settings changed from the dashboard.
    #[serde(default)]
    pub include_settings: bool,
    /// Wipe even though customers may still pay some invoices.
    #[serde(default)]
    pub force: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct WipeResult {
    pub deleted: u64,
    /// Backup taken just before wiping.
    pub backup: Backup,
}

#[derive(Debug, Clone, Default, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AuditParams {
    /// 1 to 200, default 50.
    #[serde(default)]
    pub limit: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct AuditEntry {
    pub at: u64,
    /// Hex key of the operator, or `service`.
    pub actor: String,
    pub action: String,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct AuditLog {
    pub entries: Vec<AuditEntry>,
}

// ---- Implementation ---------------------------------------------------------

impl Service {
    /// Admission for every state-changing operator tool. Returns the caller.
    fn admit_change(&self, caller: &Caller) -> Result<String> {
        self.require_admin(caller)?;
        let pubkey = caller.pubkey.clone().ok_or(Error::Unauthorized)?;
        let event = caller
            .event
            .as_ref()
            .ok_or_else(|| Error::Pow("request event unavailable".into()))?;
        self.check_event(event, &pubkey, (self.clock)(), true)?;
        Ok(pubkey)
    }

    fn read_only(&self, caller: &Caller) -> Result<()> {
        self.admit(caller)?;
        self.require_admin(caller)
    }

    fn record(&self, actor: &str, action: &str, detail: &str) -> Result<()> {
        tracing::info!(actor, action, detail, "operator action");
        self.db.audit((self.clock)(), actor, action, detail)
    }

    fn backup_dir(&self) -> PathBuf {
        self.cfg.data_dir.join(BACKUP_DIR)
    }

    /// Backups on disk, newest first.
    fn backups(&self) -> Result<Vec<(String, fs::Metadata)>> {
        let mut out = Vec::new();
        let entries = match fs::read_dir(self.backup_dir()) {
            Ok(e) => e,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(out),
            Err(e) => return Err(e.into()),
        };
        for entry in entries {
            let entry = entry?;
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with("deadhop-") && name.ends_with(".sqlite") {
                out.push((name, entry.metadata()?));
            }
        }
        // Names embed a zero-padded timestamp and sequence, so they sort by age.
        out.sort_by(|a, b| b.0.cmp(&a.0));
        Ok(out)
    }

    fn warnings(&self) -> Vec<String> {
        let mut w = self
            .settings()
            .warnings(self.cfg.network, &self.cfg.electrum, &self.cfg.electrum_security);
        if let Some(problem) = self
            .settings_problem
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
        {
            w.push(problem);
        }
        if self.is_paused() {
            w.push("invoicing is stopped: new invoices are refused until an operator starts it".into());
        }
        if let Some(file) = &self.cfg.xpub_file {
            let beside = |name| file.with_file_name(name);
            if beside(crate::keyholder::GENERATED_FILE).exists() && !beside(crate::keyholder::EXPORTED_FILE).exists() {
                w.push(
                    "the wallet was generated on this host and its recovery phrase has not been backed up: \
                     run `deadhop wallet export` in the wallet service and keep the words offline"
                        .into(),
                );
            }
        }
        w
    }

    /// Relay, chain, database and process health (operators only).
    pub async fn health(&self, caller: &Caller) -> Result<Health> {
        self.read_only(caller)?;
        let probe = self
            .relay_probe
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone();
        let relays = match probe {
            Some(p) => p.states().await,
            None => self
                .settings()
                .relays
                .iter()
                .map(|url| RelayState {
                    url: url.clone(),
                    status: "unknown".into(),
                    connected: false,
                })
                .collect(),
        };
        let now = (self.clock)();
        let chain = *self
            .chain_state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let check = self.db.check()?;
        let backups = self.backups()?;
        Ok(Health {
            service: ServiceHealth {
                version: env!("CARGO_PKG_VERSION").to_string(),
                started_at: self.started_at,
                uptime_secs: now.saturating_sub(self.started_at),
                online_since: self.online_since,
                paused: self.is_paused(),
                restarts: self.restarts.load(Ordering::Relaxed),
                restart_pending: self.restart_pending.load(Ordering::Relaxed),
            },
            relays,
            proxy: redact(&self.settings().proxy),
            chain: ChainHealth {
                ok: chain.chain_ok,
                tip_height: chain.tip,
                last_poll_at: chain.last_poll_at,
                poll_secs: self.poll_secs(),
                watch_errors: chain.watch_errors,
                backend_local: is_loopback(&self.cfg.electrum),
                transport: self.cfg.electrum_security.label().to_string(),
            },
            database: DatabaseHealth {
                ok: check.ok,
                size_bytes: check.size_bytes,
                invoices: check.invoices,
                backups: backups.len() as u32,
                last_backup_at: self.db.meta_get(META_LAST_BACKUP)?.and_then(|v| v.parse().ok()),
            },
            warnings: self.warnings(),
        })
    }

    /// Address, invoice and payment counts (operators only).
    pub fn metrics(&self, caller: &Caller) -> Result<Metrics> {
        self.read_only(caller)?;
        let now = (self.clock)();
        let t = self.db.totals()?;
        let age = now.saturating_sub(self.online_since);
        let windows = [
            ("24h", DAY_SECS),
            ("7d", 7 * DAY_SECS),
            ("30d", 30 * DAY_SECS),
            ("365d", 365 * DAY_SECS),
        ]
        .into_iter()
        .map(|(label, secs)| {
            let (count, sat) = self.db.payments_since(now.saturating_sub(secs))?;
            Ok(PaymentWindow {
                label: label.into(),
                secs,
                available: age >= secs,
                count,
                sat,
            })
        })
        .collect::<Result<Vec<_>>>()?;
        let sweep = self.settings().sweep.clone();
        Ok(Metrics {
            now,
            online_since: self.online_since,
            addresses_created: t.next_index,
            invoices: t.pending + t.detected + t.confirmed + t.expired,
            payments: t.confirmed,
            received_sat: t.confirmed_sat,
            windows,
            sweep: SweepStatus {
                enabled: sweep.enabled,
                mode: sweep.mode,
                threshold_sat: sweep.threshold_sat,
                received_sat: t.confirmed_sat,
                due: sweep.enabled && t.confirmed_sat >= sweep.threshold_sat,
            },
        })
    }

    /// Confirmed payments over time, gap-filled (operators only).
    pub fn payment_series(&self, caller: &Caller, p: SeriesParams) -> Result<Series> {
        self.read_only(caller)?;
        let (width, n) = p.range.buckets();
        let end = ((self.clock)() / width + 1) * width;
        let start = end - width * n as u64;
        let points = self
            .db
            .payment_buckets(start, width, n)?
            .into_iter()
            .enumerate()
            .map(|(i, (count, sat))| SeriesPoint {
                start: start + width * i as u64,
                count,
                sat,
            })
            .collect();
        Ok(Series {
            range: p.range,
            bucket_secs: width,
            online_since: self.online_since,
            points,
        })
    }

    fn stored_patch(&self) -> Result<SettingsPatch> {
        Ok(match self.db.meta_get(META_SETTINGS)? {
            Some(json) => serde_json::from_str(&json).unwrap_or_default(),
            None => SettingsPatch::default(),
        })
    }

    fn operators_view(&self) -> Result<Vec<Operator>> {
        let mut env: Vec<&String> = self.cfg.admins.iter().collect();
        env.sort();
        let mut out: Vec<Operator> = env
            .into_iter()
            .map(|k| Operator {
                pubkey: k.clone(),
                label: String::new(),
                source: "environment".into(),
                added_at: None,
            })
            .collect();
        out.extend(
            self.db
                .operators()?
                .into_iter()
                .filter(|o| !self.cfg.admins.contains(&o.pubkey))
                .map(|o| Operator {
                    pubkey: o.pubkey,
                    label: o.label,
                    source: "dashboard".into(),
                    added_at: Some(o.added_at),
                }),
        );
        Ok(out)
    }

    fn settings_view(&self) -> Result<SettingsView> {
        let hide = |mut s: Settings| {
            s.proxy = redact(&s.proxy);
            s
        };
        Ok(SettingsView {
            settings: hide(self.settings().clone()),
            defaults: hide(self.cfg.settings()),
            overridden: self.stored_patch()?.names(),
            fixed: FixedSettings {
                network: self.cfg.network.to_string(),
                electrum: self.cfg.electrum.clone(),
                electrum_transport: self.cfg.electrum_security.label().to_string(),
                first_address: self.first_address()?,
                announce: self.cfg.announce,
                proxy_locked: self.cfg.proxy_locked,
            },
            operators: self.operators_view()?,
            restart_pending: self.restart_pending.load(Ordering::Relaxed),
            warnings: self.warnings(),
        })
    }

    fn has_operators(&self) -> bool {
        !self.cfg.admins.is_empty()
            || !self
                .extra_operators
                .read()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .is_empty()
    }

    /// Live and default settings, and who can operate the service.
    pub fn get_settings(&self, caller: &Caller) -> Result<SettingsView> {
        self.read_only(caller)?;
        self.settings_view()
    }

    /// Change settings. Takes effect at once; a changed relay list triggers a
    /// restart (reverted automatically if none of the new relays connect).
    pub fn update_settings(&self, caller: &Caller, p: UpdateSettingsParams) -> Result<SettingsView> {
        let actor = self.admit_change(caller)?;
        if self.cfg.proxy_locked && p.patch.proxy.is_some() {
            return Err(Error::InvalidParams(
                "the proxy is locked on the host (DEADHOP_PROXY_LOCKED)".into(),
            ));
        }
        let mut stored = self.stored_patch()?;
        stored.merge(&p.patch);
        let next = stored.apply(&self.cfg.settings());
        next.validate(self.has_operators())?;
        if p.patch.relays.is_some() {
            check_relays(&next.relays, self.cfg.network)?;
        }
        let relays_changed = self.connection_changes(&next);
        let json = serde_json::to_string(&stored).expect("settings always serialize");
        self.db.meta_set(META_SETTINGS, Some(&json))?;
        self.apply_settings(next);
        *self
            .settings_problem
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = None;
        let mut shown = p.patch.clone();
        shown.proxy = shown.proxy.as_deref().map(redact);
        let changed = serde_json::to_string(&shown).expect("settings always serialize");
        self.record(&actor, "update_settings", &changed)?;
        if relays_changed {
            self.schedule_restart();
        }
        self.settings_view()
    }

    /// Drop every override and return to the environment values.
    pub fn reset_settings(&self, caller: &Caller) -> Result<SettingsView> {
        let actor = self.admit_change(caller)?;
        let next = self.cfg.settings();
        let relays_changed = self.connection_changes(&next);
        self.db.meta_set(META_SETTINGS, None)?;
        self.apply_settings(next);
        *self
            .settings_problem
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = None;
        self.record(&actor, "reset_settings", "")?;
        if relays_changed {
            self.schedule_restart();
        }
        self.settings_view()
    }

    /// Whether `next` needs new relay connections (relays or proxy differ).
    fn connection_changes(&self, next: &Settings) -> bool {
        let now = self.settings();
        next.relays != now.relays || next.proxy != now.proxy
    }

    /// Called by the supervisor when new relay connections (a relay list or
    /// a proxy) could not connect: go back to the ones that worked.
    pub fn revert_connection(&self, relays: Vec<String>, proxy: String, why: &str) -> Result<()> {
        let mut stored = self.stored_patch()?;
        stored.relays = (relays != self.cfg.relays).then_some(relays);
        stored.proxy = (proxy != self.cfg.proxy).then_some(proxy);
        let next = stored.apply(&self.cfg.settings());
        let json = serde_json::to_string(&stored).expect("settings always serialize");
        self.db.meta_set(META_SETTINGS, Some(&json))?;
        self.apply_settings(next);
        self.record("service", "revert_connection", why)
    }

    /// Let another key operate the service.
    pub fn add_operator(&self, caller: &Caller, p: AddOperatorParams) -> Result<SettingsView> {
        let actor = self.admit_change(caller)?;
        let pubkey = parse_pubkey(&p.pubkey)
            .ok_or_else(|| Error::InvalidParams("pubkey must be a hex key, npub or nprofile".into()))?;
        let label = p.label.unwrap_or_default().trim().to_string();
        if label.chars().count() > LABEL_LEN || !label.chars().all(is_plain_text) {
            return Err(Error::InvalidParams(format!(
                "label must be at most {LABEL_LEN} visible characters"
            )));
        }
        if self.cfg.admins.contains(&pubkey) {
            return Err(Error::InvalidParams(
                "that key is already an operator (from the environment)".into(),
            ));
        }
        let mut extra = self
            .extra_operators
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if !extra.contains(&pubkey) && extra.len() >= MAX_EXTRA_OPERATORS {
            return Err(Error::InvalidParams(format!(
                "at most {MAX_EXTRA_OPERATORS} operators can be added"
            )));
        }
        self.db.add_operator(&OperatorRow {
            pubkey: pubkey.clone(),
            label: label.clone(),
            added_at: (self.clock)(),
            added_by: actor.clone(),
        })?;
        extra.insert(pubkey.clone());
        drop(extra);
        self.record(&actor, "add_operator", format!("{pubkey} {label}").trim_end())?;
        self.settings_view()
    }

    /// Revoke a key added from the dashboard. Environment keys stay.
    pub fn remove_operator(&self, caller: &Caller, p: RemoveOperatorParams) -> Result<SettingsView> {
        let actor = self.admit_change(caller)?;
        let pubkey = parse_pubkey(&p.pubkey)
            .ok_or_else(|| Error::InvalidParams("pubkey must be a hex key, npub or nprofile".into()))?;
        if self.cfg.admins.contains(&pubkey) {
            return Err(Error::InvalidParams(
                "operators from DEADHOP_ADMINS can only be removed on the host".into(),
            ));
        }
        let mut extra = self
            .extra_operators
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if extra.len() == 1
            && extra.contains(&pubkey)
            && self.cfg.admins.is_empty()
            && !self.settings().public_invoicing
        {
            return Err(Error::InvalidParams(
                "this is the last operator and public invoicing is off; nobody could create invoices".into(),
            ));
        }
        if !self.db.remove_operator(&pubkey)? {
            return Err(Error::NotFound);
        }
        extra.remove(&pubkey);
        drop(extra);
        self.record(&actor, "remove_operator", &pubkey)?;
        self.settings_view()
    }

    /// Stop or start invoicing, or restart the relay connections.
    pub fn control(&self, caller: &Caller, p: ControlParams) -> Result<ControlResult> {
        let actor = self.admit_change(caller)?;
        match p.action {
            ControlAction::Stop => {
                self.db.meta_set(META_PAUSED, Some("1"))?;
                self.paused.store(true, Ordering::Relaxed);
                self.record(&actor, "stop", "invoicing stopped")?;
            }
            ControlAction::Start => {
                self.db.meta_set(META_PAUSED, None)?;
                self.paused.store(false, Ordering::Relaxed);
                self.record(&actor, "start", "invoicing started")?;
            }
            ControlAction::Restart => {
                self.record(&actor, "restart", "relay connections restarting")?;
                self.schedule_restart();
            }
        }
        Ok(ControlResult {
            paused: self.is_paused(),
            restart_pending: self.restart_pending.load(Ordering::Relaxed),
        })
    }

    fn make_backup(&self) -> Result<Backup> {
        let dir = self.backup_dir();
        fs::DirBuilder::new().recursive(true).mode(0o700).create(&dir)?;
        fs::set_permissions(&dir, fs::Permissions::from_mode(0o700))?;
        let now = (self.clock)();
        let existing = self.backups()?;
        // Several backups within one second get increasing sequence numbers.
        let seq = (0u32..)
            .find(|n| {
                !existing
                    .iter()
                    .any(|(name, _)| *name == format!("deadhop-{now:012}-{n:03}.sqlite"))
            })
            .expect("a free sequence number exists");
        let file = format!("deadhop-{now:012}-{seq:03}.sqlite");
        let path = dir.join(&file);
        self.db.backup_to(&path)?;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600))?;
        let size_bytes = fs::metadata(&path)?.len();
        self.db.meta_set(META_LAST_BACKUP, Some(&now.to_string()))?;
        for (old, _) in self.backups()?.into_iter().skip(BACKUPS_KEPT) {
            fs::remove_file(dir.join(old))?;
        }
        Ok(Backup {
            file,
            size_bytes,
            created_at: now,
        })
    }

    /// Write a consistent copy of the database next to it, on the host.
    pub fn backup(&self, caller: &Caller) -> Result<Backup> {
        let actor = self.admit_change(caller)?;
        let b = self.make_backup()?;
        self.record(&actor, "backup", &b.file)?;
        Ok(b)
    }

    /// Delete every invoice, after taking a backup. Address indices are never
    /// reused, so a wiped service still hands out fresh addresses.
    pub fn wipe(&self, caller: &Caller, p: WipeParams) -> Result<WipeResult> {
        let actor = self.admit_change(caller)?;
        if p.confirm != WIPE_CONFIRMATION {
            return Err(Error::InvalidParams(format!("type {WIPE_CONFIRMATION} to confirm")));
        }
        let now = (self.clock)();
        let watched = self
            .db
            .count_watched(now.saturating_sub(self.settings().late_grace_secs))?;
        if watched > 0 && !p.force {
            return Err(Error::InvalidParams(format!(
                "{watched} invoice(s) may still be paid; wait until they settle or pass force"
            )));
        }
        let backup = self.make_backup()?;
        let deleted = self.db.wipe(p.include_settings.then_some(META_SETTINGS))?;
        if p.include_settings {
            let next = self.cfg.settings();
            let relays_changed = self.connection_changes(&next);
            self.apply_settings(next);
            if relays_changed {
                self.schedule_restart();
            }
        }
        let detail = format!(
            "{deleted} invoices; backup {}; settings {}",
            backup.file, p.include_settings
        );
        self.record(&actor, "wipe", &detail)?;
        Ok(WipeResult { deleted, backup })
    }

    /// Recent operator actions (operators only).
    pub fn audit_log(&self, caller: &Caller, p: AuditParams) -> Result<AuditLog> {
        self.read_only(caller)?;
        let limit = p.limit.unwrap_or(50);
        if !(1..=200).contains(&limit) {
            return Err(Error::InvalidParams("limit must be between 1 and 200".into()));
        }
        let entries = self
            .db
            .audit_recent(limit)?
            .into_iter()
            .map(|r| AuditEntry {
                at: r.at,
                actor: r.actor,
                action: r.action,
                detail: r.detail,
            })
            .collect();
        Ok(AuditLog { entries })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::db::Db;
    use crate::electrum::testutil::FakeElectrum;
    use crate::pow::tests::SERVICE;
    use crate::service::tests::{public, request, rig, Rig, T0};
    use crate::service::{Clock, CreateInvoiceParams, GetInvoiceParams};
    use crate::settings::SweepPatch;
    use crate::wallet::Wallet;
    use nostr_sdk::{Keys, ToBech32};
    use std::sync::atomic::AtomicU64;
    use std::sync::Arc;

    fn code<T: std::fmt::Debug>(r: Result<T>) -> &'static str {
        r.unwrap_err().code()
    }

    fn msg<T: std::fmt::Debug>(r: Result<T>) -> String {
        r.unwrap_err().to_string()
    }

    fn patch(v: serde_json::Value) -> UpdateSettingsParams {
        UpdateSettingsParams {
            patch: serde_json::from_value(v).unwrap(),
        }
    }

    /// A caller that is not an operator.
    fn stranger() -> Caller {
        public(&Keys::generate(), 4, T0)
    }

    /// A service over a database file, so restarts can be simulated.
    async fn on_disk(cfg: &Config, path: &std::path::Path, now: u64) -> (Service, FakeElectrum) {
        let fake = FakeElectrum::start().await;
        let n = Arc::new(AtomicU64::new(now));
        let clock: Clock = Arc::new(move || n.load(Ordering::SeqCst));
        let svc = Service::new(
            cfg.clone(),
            Db::open(path).unwrap(),
            Wallet::new(&cfg.xpub, cfg.network).unwrap(),
            Arc::new(fake.client()),
            clock,
            SERVICE.to_string(),
        )
        .unwrap();
        (svc, fake)
    }

    fn admin_of(keys: &Keys, at: u64, salt: u64) -> Caller {
        Caller {
            pubkey: Some(keys.public_key().to_hex()),
            event: Some(request(keys, at, salt)),
        }
    }

    fn pay_and_confirm(r: &Rig, amount: u64) {
        let inv = r.create(amount);
        let script = r.svc.wallet.script_for(&inv.address).unwrap();
        r.fake.pay(&script, amount as i64 as u64, 100);
        r.fake.set(|s| s.tip = 101);
    }

    struct Probe(Vec<RelayState>);
    #[async_trait::async_trait]
    impl crate::service::RelayProbe for Probe {
        async fn states(&self) -> Vec<RelayState> {
            self.0.clone()
        }
    }

    #[tokio::test]
    async fn operator_tools_are_closed_to_everyone_else() {
        let r = rig(|_| {}).await;
        let s = stranger();
        assert_eq!(code(r.svc.health(&s).await), "unauthorized");
        assert_eq!(code(r.svc.metrics(&s)), "unauthorized");
        assert_eq!(
            code(r.svc.payment_series(&s, SeriesParams { range: Range::Day })),
            "unauthorized"
        );
        assert_eq!(code(r.svc.get_settings(&s)), "unauthorized");
        assert_eq!(
            code(r.svc.update_settings(&s, patch(serde_json::json!({})))),
            "unauthorized"
        );
        assert_eq!(code(r.svc.reset_settings(&s)), "unauthorized");
        let add = AddOperatorParams {
            pubkey: s.pubkey.clone().unwrap(),
            label: None,
        };
        assert_eq!(code(r.svc.add_operator(&s, add)), "unauthorized");
        let rm = RemoveOperatorParams {
            pubkey: s.pubkey.clone().unwrap(),
        };
        assert_eq!(code(r.svc.remove_operator(&s, rm)), "unauthorized");
        let stop = ControlParams {
            action: ControlAction::Stop,
        };
        assert_eq!(code(r.svc.control(&s, stop)), "unauthorized");
        assert_eq!(code(r.svc.backup(&s)), "unauthorized");
        let wipe = WipeParams {
            confirm: "WIPE".into(),
            include_settings: false,
            force: true,
        };
        assert_eq!(code(r.svc.wipe(&s, wipe)), "unauthorized");
        assert_eq!(code(r.svc.audit_log(&s, AuditParams::default())), "unauthorized");
    }

    #[tokio::test]
    async fn changes_need_a_fresh_unused_request_even_across_restarts() {
        let r = rig(|_| {}).await;
        let caller = r.admin();
        r.svc
            .control(
                &caller,
                ControlParams {
                    action: ControlAction::Stop,
                },
            )
            .unwrap();
        let again = r.svc.control(
            &caller,
            ControlParams {
                action: ControlAction::Start,
            },
        );
        assert!(msg(again).contains("already used"));
        let no_event = Caller {
            event: None,
            ..r.admin()
        };
        assert_eq!(code(r.svc.backup(&no_event)), "pow_rejected");
        let no_key = Caller {
            pubkey: None,
            ..r.admin()
        };
        assert_eq!(code(r.svc.backup(&no_key)), "unauthorized");
        let stale = admin_of(&r.admin_keys, T0 - 10_000, 1);
        assert_eq!(code(r.svc.backup(&stale)), "pow_rejected");

        // The used id is in the database, so a fresh process refuses it too.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("db.sqlite");
        let cfg = r.svc.cfg.clone();
        let event = admin_of(&r.admin_keys, T0, 77);
        let (first, _f1) = on_disk(&cfg, &path, T0).await;
        first.backup(&event).unwrap();
        drop(first);
        let (second, _f2) = on_disk(&cfg, &path, T0).await;
        assert!(msg(second.backup(&event)).contains("already used"));
    }

    #[tokio::test]
    async fn an_unexported_generated_wallet_is_a_warning() {
        let public = tempfile::tempdir().unwrap();
        let file = public.path().join("account.xpub");
        let f = file.clone();
        let r = rig(move |c| c.xpub_file = Some(f)).await;
        let warned = |r: &Rig| {
            let w = r.svc.warnings();
            w.iter().any(|w| w.contains("recovery phrase has not been backed up"))
        };
        assert!(!warned(&r), "a key from a file is not necessarily generated here");
        fs::write(file.with_file_name(crate::keyholder::GENERATED_FILE), "generated").unwrap();
        assert!(warned(&r));
        fs::write(file.with_file_name(crate::keyholder::EXPORTED_FILE), "1").unwrap();
        assert!(!warned(&r));
    }

    #[tokio::test]
    async fn health_reports_every_part() {
        let r = rig(|_| {}).await;
        let h = r.svc.health(&r.admin()).await.unwrap();
        assert_eq!(h.relays.len(), 2);
        assert!(h.relays.iter().all(|x| x.status == "unknown" && !x.connected));
        assert!(h.database.ok);
        assert_eq!((h.database.backups, h.database.last_backup_at), (0, None));
        assert!(!h.chain.ok && h.chain.backend_local);
        assert_eq!(h.chain.poll_secs, 10);
        assert_eq!(
            (h.service.started_at, h.service.uptime_secs, h.service.online_since),
            (T0, 0, T0)
        );
        assert!(!h.service.paused);
        assert_eq!(h.warnings.len(), 1, "the rig's weak proof of work: {:?}", h.warnings);

        r.svc.poll_once().await.unwrap();
        r.svc.set_relay_probe(Some(Arc::new(Probe(vec![RelayState {
            url: "wss://relay.example".into(),
            status: "connected".into(),
            connected: true,
        }]))));
        r.svc
            .control(
                &r.admin(),
                ControlParams {
                    action: ControlAction::Stop,
                },
            )
            .unwrap();
        r.svc.backup(&r.admin()).unwrap();
        r.set_now(T0 + 90);
        let h = r.svc.health(&r.admin()).await.unwrap();
        assert!(h.relays[0].connected);
        assert!(h.chain.ok && h.chain.last_poll_at.is_some());
        assert_eq!((h.database.backups, h.database.last_backup_at), (1, Some(T0)));
        assert_eq!(h.service.uptime_secs, 90);
        assert!(h.service.paused);
        assert!(h.warnings.iter().any(|w| w.contains("invoicing is stopped")));

        // A backup location that cannot be listed is an error, not a crash.
        let dir = r.svc.cfg.data_dir.join(BACKUP_DIR);
        fs::remove_dir_all(&dir).unwrap();
        fs::write(&dir, b"not a directory").unwrap();
        assert_eq!(code(r.svc.health(&r.admin()).await), "internal");
    }

    #[tokio::test]
    async fn metrics_windows_are_gated_by_how_long_the_service_has_run() {
        let r = rig(|_| {}).await;
        r.svc
            .update_settings(
                &r.admin(),
                patch(serde_json::json!({ "sweep": { "enabled": true, "threshold_sat": 10000 } })),
            )
            .unwrap();
        pay_and_confirm(&r, 6_000);
        r.set_now(T0 + 2 * DAY_SECS);
        r.svc.poll_once().await.unwrap();
        pay_and_confirm(&r, 5_000);
        r.svc.poll_once().await.unwrap();
        r.create(7_000);

        let m = r.svc.metrics(&r.admin()).unwrap();
        assert_eq!(
            (m.addresses_created, m.invoices, m.payments, m.received_sat),
            (3, 3, 2, 11_000)
        );
        assert_eq!((m.now, m.online_since), (T0 + 2 * DAY_SECS, T0));
        let w: Vec<(&str, bool, u64, u64)> = m
            .windows
            .iter()
            .map(|w| (w.label.as_str(), w.available, w.count, w.sat))
            .collect();
        assert_eq!(
            w,
            [
                ("24h", true, 2, 11_000),
                ("7d", false, 2, 11_000),
                ("30d", false, 2, 11_000),
                ("365d", false, 2, 11_000)
            ]
        );
        assert!(m.sweep.enabled && m.sweep.due && m.sweep.received_sat == 11_000);

        // Hourly buckets for a day, gap-filled, ending with the current hour.
        let day = r
            .svc
            .payment_series(&r.admin(), SeriesParams { range: Range::Day })
            .unwrap();
        assert_eq!((day.points.len(), day.bucket_secs, day.online_since), (24, 3_600, T0));
        assert_eq!(day.points.iter().map(|p| p.count).sum::<u64>(), 2);
        let last = day.points.last().unwrap();
        assert!(last.start <= T0 + 2 * DAY_SECS && T0 + 2 * DAY_SECS < last.start + 3_600);
        assert_eq!(last.count, 2);
        for (range, n, width) in [
            (Range::Week, 7, DAY_SECS),
            (Range::Month, 30, DAY_SECS),
            (Range::Year, 52, 7 * DAY_SECS),
        ] {
            let s = r.svc.payment_series(&r.admin(), SeriesParams { range }).unwrap();
            assert_eq!((s.points.len(), s.bucket_secs, s.range), (n, width, range));
            assert_eq!(s.points.iter().map(|p| p.sat).sum::<u64>(), 11_000);
        }
    }

    #[tokio::test]
    async fn settings_change_live_persist_and_reset() {
        let r = rig(|_| {}).await;
        let v = r.svc.get_settings(&r.admin()).unwrap();
        assert!(v.overridden.is_empty() && !v.restart_pending);
        assert_eq!(v.settings, v.defaults);
        assert_eq!((v.fixed.network.as_str(), v.fixed.announce), ("regtest", false));
        assert_eq!(v.fixed.first_address, r.svc.first_address().unwrap());
        assert_eq!(v.operators.len(), 1);
        assert_eq!(v.operators[0].source, "environment");

        let v = r
            .svc
            .update_settings(
                &r.admin(),
                patch(serde_json::json!({ "pow_bits": 12, "name": "Shop", "min_confirmations": 3 })),
            )
            .unwrap();
        assert_eq!(v.overridden, ["name", "pow_bits", "min_confirmations"]);
        assert!(!v.restart_pending, "no relay change, no restart");
        let info = r.svc.info(&stranger()).unwrap();
        assert_eq!(
            (info.pow_bits, info.name.as_str(), info.min_confirmations),
            (12, "Shop", 3)
        );
        assert_eq!(r.svc.pow.bits(), 12);

        // Invalid changes are refused and leave everything as it was.
        let bad = r.svc.update_settings(
            &r.admin(),
            patch(serde_json::json!({ "min_amount_sat": 10_000_000_000u64 })),
        );
        assert!(msg(bad).contains("min_amount_sat exceeds"));
        assert_eq!(r.svc.settings().pow_bits, 12);

        // A new relay list is applied by a restart.
        let v = r
            .svc
            .update_settings(
                &r.admin(),
                patch(serde_json::json!({ "relays": ["wss://new.example"] })),
            )
            .unwrap();
        assert!(v.restart_pending);
        tokio::time::timeout(std::time::Duration::from_secs(1), r.svc.restart_requested())
            .await
            .unwrap();
        r.svc.restarted();
        assert!(!r.svc.get_settings(&r.admin()).unwrap().restart_pending);

        // Going back to the environment, which also restarts for the relays.
        let v = r.svc.reset_settings(&r.admin()).unwrap();
        assert!(v.overridden.is_empty() && v.restart_pending);
        assert_eq!(r.svc.settings().pow_bits, 4);
        r.svc.restarted();
        let v = r.svc.reset_settings(&r.admin()).unwrap();
        assert!(!v.restart_pending, "nothing to reconnect");

        let log = r.svc.audit_log(&r.admin(), AuditParams { limit: Some(10) }).unwrap();
        let actions: Vec<&str> = log.entries.iter().map(|e| e.action.as_str()).collect();
        assert_eq!(
            actions,
            ["reset_settings", "reset_settings", "update_settings", "update_settings"]
        );
        assert_eq!(log.entries[3].actor, r.admin_keys.public_key().to_hex());
        assert!(log.entries[3].detail.contains("\"pow_bits\":12"));
        assert_eq!(
            code(r.svc.audit_log(&r.admin(), AuditParams { limit: Some(0) })),
            "invalid_params"
        );
        assert_eq!(
            r.svc
                .audit_log(&r.admin(), AuditParams::default())
                .unwrap()
                .entries
                .len(),
            4
        );
    }

    #[tokio::test]
    async fn stored_settings_survive_restarts_unless_they_no_longer_fit() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("db.sqlite");
        let keys = Keys::generate();
        let mut cfg = crate::config::tests::load(&crate::config::tests::base()).unwrap();
        cfg.admins = [keys.public_key().to_hex()].into();
        cfg.data_dir = dir.path().to_path_buf();
        {
            let (svc, _f) = on_disk(&cfg, &path, T0).await;
            svc.update_settings(
                &admin_of(&keys, T0, 1),
                patch(serde_json::json!({ "default_expiry_secs": 7200, "sweep": { "mode": "coinjoin" } })),
            )
            .unwrap();
            svc.control(
                &admin_of(&keys, T0, 2),
                ControlParams {
                    action: ControlAction::Stop,
                },
            )
            .unwrap();
        }
        let (svc, _f) = on_disk(&cfg, &path, T0 + 5).await;
        assert_eq!(svc.settings().default_expiry_secs, 7200);
        assert_eq!(svc.settings().sweep.mode, SweepMode::Coinjoin);
        assert!(svc.is_paused(), "stop survives a restart");
        assert_eq!(svc.online_since, T0);
        drop(svc);

        // The environment changed so that the stored value is out of range.
        let mut tight = cfg.clone();
        tight.max_expiry_secs = 3_600;
        let (svc, _f) = on_disk(&tight, &path, T0).await;
        assert_eq!(svc.settings().default_expiry_secs, 3_600);
        let w = svc.health(&admin_of(&keys, T0, 3)).await.unwrap().warnings;
        assert!(w.iter().any(|w| w.contains("saved settings were not applied")), "{w:?}");
        // Saving anything valid clears the warning.
        svc.update_settings(
            &admin_of(&keys, T0, 4),
            patch(serde_json::json!({ "default_expiry_secs": 600 })),
        )
        .unwrap();
        assert!(svc
            .get_settings(&admin_of(&keys, T0, 5))
            .unwrap()
            .warnings
            .iter()
            .all(|w| !w.contains("saved")));
        drop(svc);

        // Unreadable stored settings are ignored the same way.
        Db::open(&path)
            .unwrap()
            .meta_set(META_SETTINGS, Some("{not json"))
            .unwrap();
        let (svc, _f) = on_disk(&cfg, &path, T0).await;
        assert_eq!(svc.settings().default_expiry_secs, 3_600);
        assert!(svc
            .settings_problem
            .lock()
            .unwrap()
            .as_ref()
            .unwrap()
            .contains("could not be read"));
        // A reset also clears the problem.
        svc.reset_settings(&admin_of(&keys, T0, 6)).unwrap();
        assert!(svc.settings_problem.lock().unwrap().is_none());
    }

    #[tokio::test]
    async fn operator_keys_can_be_added_in_any_format_and_removed() {
        let r = rig(|_| {}).await;
        let new = Keys::generate();
        let hex = new.public_key().to_hex();
        let profile = nostr_sdk::nips::nip19::Nip19Profile::new(
            new.public_key(),
            [nostr_sdk::RelayUrl::parse("wss://relay.example").unwrap()],
        );
        let forms = [
            hex.clone(),
            new.public_key().to_bech32().unwrap(),
            format!("nostr:{}", profile.to_bech32().unwrap()),
        ];
        for form in &forms {
            let v = r
                .svc
                .add_operator(
                    &r.admin(),
                    AddOperatorParams {
                        pubkey: form.clone(),
                        label: Some(" till 2 ".into()),
                    },
                )
                .unwrap();
            assert_eq!(v.operators.len(), 2);
            assert_eq!(
                (v.operators[1].pubkey.as_str(), v.operators[1].label.as_str()),
                (hex.as_str(), "till 2")
            );
            assert_eq!(v.operators[1].source, "dashboard");
        }
        // The new key is an operator now.
        let as_new = admin_of(&new, T0, 1);
        assert!(r.svc.stats(&as_new).is_ok());

        let bad = |pubkey: &str, label: Option<&str>| {
            msg(r.svc.add_operator(
                &r.admin(),
                AddOperatorParams {
                    pubkey: pubkey.into(),
                    label: label.map(Into::into),
                },
            ))
        };
        assert!(bad("npub1nope", None).contains("hex key, npub or nprofile"));
        assert!(bad(&hex, Some("a\u{202E}b")).contains("label"));
        assert!(bad(&r.admin_keys.public_key().to_hex(), None).contains("already an operator"));
        for _ in 1..MAX_EXTRA_OPERATORS {
            r.svc
                .add_operator(
                    &r.admin(),
                    AddOperatorParams {
                        pubkey: Keys::generate().public_key().to_hex(),
                        label: None,
                    },
                )
                .unwrap();
        }
        assert!(bad(&Keys::generate().public_key().to_hex(), None).contains("at most 20"));
        // Relabelling an existing one still works at the limit.
        r.svc
            .add_operator(
                &r.admin(),
                AddOperatorParams {
                    pubkey: hex.clone(),
                    label: None,
                },
            )
            .unwrap();

        let rm = |pubkey: &str| {
            r.svc
                .remove_operator(&r.admin(), RemoveOperatorParams { pubkey: pubkey.into() })
        };
        assert!(msg(rm(&r.admin_keys.public_key().to_hex())).contains("only be removed on the host"));
        assert!(msg(rm("x")).contains("hex key"));
        assert_eq!(code(rm(&Keys::generate().public_key().to_hex())), "not_found");
        rm(&new.public_key().to_bech32().unwrap()).unwrap();
        assert_eq!(code(r.svc.stats(&admin_of(&new, T0, 2))), "unauthorized");
        let actions: Vec<String> = r
            .svc
            .audit_log(&r.admin(), AuditParams::default())
            .unwrap()
            .entries
            .into_iter()
            .map(|e| e.action)
            .collect();
        assert_eq!(actions[0], "remove_operator");
        assert!(actions.iter().filter(|a| *a == "add_operator").count() >= 3);
    }

    #[tokio::test]
    async fn the_last_operator_cannot_lock_everyone_out() {
        // No operators in the environment: the public opened the service.
        let r = rig(|c| c.admins.clear()).await;
        let first = Keys::generate();
        // Seed one operator directly, as the host would.
        r.svc
            .db
            .add_operator(&OperatorRow {
                pubkey: first.public_key().to_hex(),
                label: String::new(),
                added_at: T0,
                added_by: "host".into(),
            })
            .unwrap();
        r.svc
            .extra_operators
            .write()
            .unwrap()
            .insert(first.public_key().to_hex());
        r.svc
            .update_settings(
                &admin_of(&first, T0, 1),
                patch(serde_json::json!({ "public_invoicing": false })),
            )
            .unwrap();
        let out = r.svc.remove_operator(
            &admin_of(&first, T0, 2),
            RemoveOperatorParams {
                pubkey: first.public_key().to_hex(),
            },
        );
        assert!(msg(out).contains("last operator"));
        // With public invoicing on again, the last operator may leave.
        r.svc
            .update_settings(
                &admin_of(&first, T0, 3),
                patch(serde_json::json!({ "public_invoicing": true })),
            )
            .unwrap();
        r.svc
            .remove_operator(
                &admin_of(&first, T0, 4),
                RemoveOperatorParams {
                    pubkey: first.public_key().to_hex(),
                },
            )
            .unwrap();
        assert!(msg(r
            .svc
            .update_settings(&admin_of(&first, T0, 5), patch(serde_json::json!({}))))
        .contains("not authorized"));
    }

    #[tokio::test]
    async fn stop_start_and_restart() {
        let r = rig(|_| {}).await;
        let c = r
            .svc
            .control(
                &r.admin(),
                ControlParams {
                    action: ControlAction::Stop,
                },
            )
            .unwrap();
        assert!(c.paused && !c.restart_pending);
        let p = CreateInvoiceParams {
            amount_sat: 5_000,
            memo: None,
            expiry_secs: None,
        };
        assert_eq!(code(r.svc.create_invoice(&r.admin(), p.clone())), "paused");
        assert_eq!(
            code(r.svc.create_invoice(&public(&Keys::generate(), 4, T0), p.clone())),
            "paused"
        );
        // Existing invoices can still be looked up and are still watched.
        let c = r
            .svc
            .control(
                &r.admin(),
                ControlParams {
                    action: ControlAction::Start,
                },
            )
            .unwrap();
        assert!(!c.paused);
        let inv = r.svc.create_invoice(&r.admin(), p).unwrap();
        r.svc
            .control(
                &r.admin(),
                ControlParams {
                    action: ControlAction::Stop,
                },
            )
            .unwrap();
        assert!(r
            .svc
            .get_invoice(&stranger(), GetInvoiceParams { id: inv.id.clone() })
            .is_ok());
        assert_eq!(r.svc.poll_once().await.unwrap(), 0);

        let c = r
            .svc
            .control(
                &r.admin(),
                ControlParams {
                    action: ControlAction::Restart,
                },
            )
            .unwrap();
        assert!(c.restart_pending);
        r.svc.restart_requested().await;
        r.svc.restarted();
        let h = r.svc.health(&r.admin()).await.unwrap();
        assert_eq!((h.service.restarts, h.service.restart_pending), (1, false));
    }

    #[tokio::test]
    async fn relays_are_reverted_on_request_of_the_supervisor() {
        let r = rig(|_| {}).await;
        let env = r.svc.cfg.relays.clone();
        r.svc
            .update_settings(&r.admin(), patch(serde_json::json!({ "relays": ["wss://a.example"] })))
            .unwrap();
        r.svc
            .update_settings(&r.admin(), patch(serde_json::json!({ "relays": ["wss://b.example"] })))
            .unwrap();
        r.svc
            .revert_connection(vec!["wss://a.example".into()], String::new(), "test")
            .unwrap();
        assert_eq!(r.svc.settings().relays, ["wss://a.example"]);
        assert!(r
            .svc
            .get_settings(&r.admin())
            .unwrap()
            .overridden
            .contains(&"relays".to_string()));
        // Reverting to the environment list removes the override.
        r.svc.revert_connection(env.clone(), String::new(), "test").unwrap();
        assert_eq!(r.svc.settings().relays, env);
        assert!(r.svc.get_settings(&r.admin()).unwrap().overridden.is_empty());
        let log = r.svc.audit_log(&r.admin(), AuditParams::default()).unwrap();
        assert_eq!(
            (log.entries[0].actor.as_str(), log.entries[0].action.as_str()),
            ("service", "revert_connection")
        );
    }

    #[tokio::test]
    async fn backups_are_private_numbered_and_pruned() {
        use std::os::unix::fs::PermissionsExt;
        let r = rig(|_| {}).await;
        r.create(5_000);
        let b = r.svc.backup(&r.admin()).unwrap();
        assert_eq!(b.file, format!("deadhop-{T0:012}-000.sqlite"));
        assert!(b.size_bytes > 0 && b.created_at == T0);
        let dir = r.svc.cfg.data_dir.join(BACKUP_DIR);
        assert_eq!(fs::metadata(&dir).unwrap().permissions().mode() & 0o777, 0o700);
        let path = dir.join(&b.file);
        assert_eq!(fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600);
        assert_eq!(Db::open(&path).unwrap().totals().unwrap().pending, 1);

        // Same second: next sequence number. Unrelated files are left alone.
        fs::write(dir.join("notes.txt"), b"keep").unwrap();
        assert_eq!(
            r.svc.backup(&r.admin()).unwrap().file,
            format!("deadhop-{T0:012}-001.sqlite")
        );
        for i in 0..12 {
            r.set_now(T0 + 1 + i);
            r.svc.backup(&r.admin()).unwrap();
        }
        let left: Vec<String> = r.svc.backups().unwrap().into_iter().map(|b| b.0).collect();
        assert_eq!(left.len(), BACKUPS_KEPT);
        assert_eq!(left[0], format!("deadhop-{:012}-000.sqlite", T0 + 12));
        assert!(dir.join("notes.txt").exists());

        // A data directory that cannot hold backups.
        fs::remove_dir_all(&dir).unwrap();
        fs::write(&dir, b"x").unwrap();
        assert_eq!(code(r.svc.backup(&r.admin())), "internal");
    }

    #[tokio::test]
    async fn wiping_is_confirmed_backed_up_and_never_reuses_addresses() {
        let r = rig(|_| {}).await;
        let first = r.create(5_000);
        r.svc
            .update_settings(&r.admin(), patch(serde_json::json!({ "pow_bits": 9 })))
            .unwrap();
        let wipe = |confirm: &str, include_settings, force| {
            r.svc.wipe(
                &r.admin(),
                WipeParams {
                    confirm: confirm.into(),
                    include_settings,
                    force,
                },
            )
        };
        assert!(msg(wipe("wipe", false, true)).contains("type WIPE"));
        assert!(msg(wipe("WIPE", false, false)).contains("1 invoice(s) may still be paid"));
        let w = wipe("WIPE", false, true).unwrap();
        assert_eq!(w.deleted, 1);
        assert!(r.svc.cfg.data_dir.join(BACKUP_DIR).join(&w.backup.file).exists());
        assert_eq!(r.svc.settings().pow_bits, 9, "settings kept");
        let next = r.create(5_000);
        assert_ne!(next.address, first.address);
        assert_eq!(r.svc.metrics(&r.admin()).unwrap().addresses_created, 2);

        // Nothing watched any more: no force needed. Settings can go too.
        r.svc
            .db
            .update(&next.id, crate::db::Status::Expired, 0, 0, None)
            .unwrap();
        r.set_now(T0 + 30 * DAY_SECS);
        let w = wipe("WIPE", true, false).unwrap();
        assert_eq!(w.deleted, 1);
        assert_eq!(r.svc.settings().pow_bits, 4, "settings back to the environment");
        let log = r.svc.audit_log(&r.admin(), AuditParams::default()).unwrap();
        assert!(log.entries[0].detail.contains("settings true"));

        // Wiping settings that changed the relays reconnects.
        r.svc
            .update_settings(&r.admin(), patch(serde_json::json!({ "relays": ["wss://x.example"] })))
            .unwrap();
        r.svc.restarted();
        wipe("WIPE", true, false).unwrap();
        assert!(r.svc.restart_pending.load(Ordering::Relaxed));

        // A failing backup stops the wipe.
        let dir = r.svc.cfg.data_dir.join(BACKUP_DIR);
        fs::remove_dir_all(&dir).unwrap();
        fs::write(&dir, b"x").unwrap();
        let _ = r.create(5_000);
        assert_eq!(code(wipe("WIPE", false, true)), "internal");
        assert_eq!(r.svc.metrics(&r.admin()).unwrap().invoices, 1);
    }

    #[tokio::test]
    async fn an_environment_relay_list_is_not_rechecked_on_unrelated_changes() {
        // Accepted as configured on the host (for example ws:// on mainnet, which
        // only warns), but the same list would be refused from the dashboard.
        let r = rig(|c| c.relays = vec!["ws://".into()]).await;
        r.svc
            .update_settings(&r.admin(), patch(serde_json::json!({ "pow_bits": 21 })))
            .unwrap();
        let out = r
            .svc
            .update_settings(&r.admin(), patch(serde_json::json!({ "relays": ["ws://"] })));
        assert!(msg(out).contains("not a valid url"));
    }

    #[tokio::test]
    async fn stored_relays_are_checked_at_startup() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("db.sqlite");
        let cfg = crate::config::tests::load(&crate::config::tests::base()).unwrap();
        Db::open(&path)
            .unwrap()
            .meta_set(META_SETTINGS, Some(r#"{"relays":["https://x"]}"#))
            .unwrap();
        let (svc, _f) = on_disk(&cfg, &path, T0).await;
        assert_eq!(svc.settings().relays, cfg.relays);
        assert!(svc
            .settings_problem
            .lock()
            .unwrap()
            .as_ref()
            .unwrap()
            .contains("ws:// or wss://"));
    }

    #[tokio::test]
    async fn proxy_passwords_stay_hidden_and_the_host_can_lock_the_proxy() {
        let r = rig(|_| {}).await;
        let v = r
            .svc
            .update_settings(
                &r.admin(),
                patch(serde_json::json!({ "proxy": "socks5://me:hunter2@127.0.0.1:9050" })),
            )
            .unwrap();
        assert!(v.restart_pending, "a new proxy means new connections");
        assert_eq!(v.settings.proxy, "socks5://me:***@127.0.0.1:9050");
        assert!(!v.fixed.proxy_locked);
        assert_eq!(
            r.svc.settings().proxy,
            "socks5://me:hunter2@127.0.0.1:9050",
            "the live value is intact"
        );
        let log = r.svc.audit_log(&r.admin(), AuditParams::default()).unwrap();
        assert!(!log.entries[0].detail.contains("hunter2"), "{}", log.entries[0].detail);
        assert_eq!(
            r.svc.health(&r.admin()).await.unwrap().proxy,
            "socks5://me:***@127.0.0.1:9050"
        );
        assert!(msg(r
            .svc
            .update_settings(&r.admin(), patch(serde_json::json!({ "proxy": "ftp://x:1" }))))
        .contains("scheme"));
        r.svc
            .revert_connection(r.svc.cfg.relays.clone(), "socks5://127.0.0.1:1".into(), "test")
            .unwrap();
        assert!(r
            .svc
            .get_settings(&r.admin())
            .unwrap()
            .overridden
            .contains(&"proxy".to_string()));

        let locked = rig(|c| c.proxy_locked = true).await;
        let out = locked
            .svc
            .update_settings(&locked.admin(), patch(serde_json::json!({ "proxy": "" })));
        assert!(msg(out).contains("locked on the host"));
        assert!(locked.svc.get_settings(&locked.admin()).unwrap().fixed.proxy_locked);
        // Everything else can still be changed.
        locked
            .svc
            .update_settings(&locked.admin(), patch(serde_json::json!({ "pow_bits": 10 })))
            .unwrap();
    }

    #[tokio::test]
    async fn an_existing_database_counts_as_online_since_its_first_invoice() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("db.sqlite");
        let keys = Keys::generate();
        let mut cfg = crate::config::tests::load(&crate::config::tests::base()).unwrap();
        cfg.admins = [keys.public_key().to_hex()].into();
        {
            let (svc, _f) = on_disk(&cfg, &path, T0 - 500).await;
            let p = CreateInvoiceParams {
                amount_sat: 5_000,
                memo: None,
                expiry_secs: None,
            };
            svc.create_invoice(&admin_of(&keys, T0 - 500, 1), p).unwrap();
        }
        // As if written by a version that did not record the first start.
        Db::open(&path)
            .unwrap()
            .meta_set(crate::service::META_ONLINE_SINCE, None)
            .unwrap();
        let (svc, _f) = on_disk(&cfg, &path, T0).await;
        assert_eq!(svc.online_since, T0 - 500);
    }

    #[test]
    fn sweep_patch_unknown_fields_are_rejected() {
        assert!(serde_json::from_value::<SweepPatch>(serde_json::json!({ "destination": "bc1q" })).is_err());
    }
}
