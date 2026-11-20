//! Business logic, independent of the transport that carries requests.

use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex, RwLock, RwLockReadGuard};

use nostr_sdk::Event;
use rmcp::schemars::{self, JsonSchema};
use serde::{Deserialize, Serialize};

use crate::config::{Config, MAX_MONEY_SAT};
use crate::db::{Db, InvoiceRow, NewInvoice, Status};
use crate::electrum::{Chain, Payment, ScriptOutcome};
use crate::error::{Error, Result};
use crate::pow::PowGuard;
use crate::ratelimit::RateLimiter;
use crate::settings::{Settings, SettingsPatch};
use crate::wallet::{bip21, Wallet};

/// Longest accepted memo, in characters.
pub const MAX_MEMO_CHARS: usize = 200;
/// Largest page `list_invoices` returns.
pub const MAX_PAGE: u32 = 200;
/// Scripts queried per chain round trip.
const POLL_CHUNK: usize = 50;
/// Request events remembered for replay protection.
const POW_CACHE: usize = 100_000;
/// Window of the per-day invoice cap, in seconds.
pub(crate) const DAY_SECS: u64 = 86_400;
/// `meta` keys.
pub(crate) const META_SETTINGS: &str = "settings";
pub(crate) const META_PAUSED: &str = "paused";
pub(crate) const META_ONLINE_SINCE: &str = "online_since";
pub(crate) const META_LAST_BACKUP: &str = "last_backup_at";

/// Source of the current unix time (injectable for tests).
pub type Clock = Arc<dyn Fn() -> u64 + Send + Sync>;

/// Wall-clock time.
pub fn system_clock() -> Clock {
    Arc::new(|| {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs())
    })
}

/// Who is asking, as established by the transport.
#[derive(Debug, Clone, Default)]
pub struct Caller {
    /// Hex public key that signed the request.
    pub pubkey: Option<String>,
    /// The signed request event itself.
    pub event: Option<Event>,
}

/// Arguments of `create_invoice`.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CreateInvoiceParams {
    /// Amount to request, in satoshis.
    pub amount_sat: u64,
    /// Optional note stored with the invoice (max 200 characters).
    #[serde(default)]
    pub memo: Option<String>,
    /// Optional lifetime in seconds.
    #[serde(default)]
    pub expiry_secs: Option<u64>,
}

/// Arguments of `get_invoice`.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GetInvoiceParams {
    /// Invoice id: 32 lowercase hex characters.
    pub id: String,
}

/// Arguments of `list_invoices`.
#[derive(Debug, Clone, Default, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ListInvoicesParams {
    /// Only invoices in this state.
    #[serde(default)]
    pub status: Option<Status>,
    /// Only invoices with a derivation index below this (pagination cursor).
    #[serde(default)]
    pub before: Option<u32>,
    /// Page size, 1 to 200 (default 50).
    #[serde(default)]
    pub limit: Option<u32>,
}

/// An invoice as seen by whoever holds its id.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct Invoice {
    pub id: String,
    pub address: String,
    pub amount_sat: u64,
    pub memo: String,
    pub status: Status,
    pub received_sat: u64,
    pub confirmed_sat: u64,
    pub created_at: u64,
    pub expires_at: u64,
    pub settled_at: Option<u64>,
    pub bip21: String,
    pub confirmations_required: u32,
}

/// An invoice with the fields only the operator may see.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct AdminInvoice {
    #[serde(flatten)]
    pub invoice: Invoice,
    pub index: u32,
    pub creator: String,
}

/// Result of `list_invoices`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct InvoiceList {
    pub invoices: Vec<AdminInvoice>,
    /// Pass as `before` to fetch the next page; absent on the last page.
    pub next_before: Option<u32>,
}

/// Result of `get_info`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct Info {
    pub name: String,
    pub version: String,
    pub network: String,
    pub public_invoicing: bool,
    pub pow_bits: u8,
    pub pow_window_secs: u64,
    pub min_confirmations: u32,
    pub default_expiry_secs: u64,
    /// Longest lifetime this caller may request.
    pub max_expiry_secs: u64,
    pub min_amount_sat: u64,
    pub max_amount_sat: u64,
    /// Whether the caller is an operator.
    pub is_admin: bool,
}

/// Result of `get_stats`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct Stats {
    pub pending: u64,
    pub detected: u64,
    pub confirmed: u64,
    pub expired: u64,
    pub confirmed_sat: u64,
    pub detected_sat: u64,
    pub next_index: u32,
    pub tip_height: Option<u32>,
    pub last_poll_at: Option<u64>,
    pub chain_ok: bool,
    /// Invoices whose payments could not be read in the last poll and need a look.
    pub watch_errors: u64,
}

#[derive(Default, Clone, Copy)]
pub(crate) struct ChainState {
    pub(crate) tip: Option<u32>,
    pub(crate) last_poll_at: Option<u64>,
    pub(crate) chain_ok: bool,
    pub(crate) watch_errors: u64,
}

/// What one poll found.
struct PollReport {
    changed: usize,
    tip: Option<u32>,
    watch_errors: u64,
    failure: Option<Error>,
}

/// Connection state of one relay, as reported by the transport.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct RelayState {
    pub url: String,
    /// Transport status, for example `connected`, `connecting` or `disconnected`.
    pub status: String,
    pub connected: bool,
}

/// Something that can report relay connections (the live relay pool).
#[async_trait::async_trait]
pub trait RelayProbe: Send + Sync {
    async fn states(&self) -> Vec<RelayState>;
}

/// The payment service.
pub struct Service {
    /// Static configuration. Its runtime-changeable fields are only the
    /// environment defaults; the live values are in `settings`.
    pub(crate) cfg: Config,
    pub(crate) db: Db,
    pub(crate) wallet: Wallet,
    chain: Arc<dyn Chain>,
    pub(crate) pow: PowGuard,
    /// Budget for invoice creation by the public.
    create_limiter: RateLimiter,
    /// Budget for any tool call by the public.
    request_limiter: RateLimiter,
    pub(crate) clock: Clock,
    pub(crate) chain_state: Mutex<ChainState>,
    /// Live settings: environment defaults plus stored overrides.
    settings: RwLock<Settings>,
    /// Why stored overrides were not applied at startup, if they were not.
    pub(crate) settings_problem: Mutex<Option<String>>,
    /// Operator keys added from the dashboard (environment ones are in `cfg`).
    pub(crate) extra_operators: RwLock<HashSet<String>>,
    /// Refuse new invoices (operator "stop"). Chain watching continues.
    pub(crate) paused: AtomicBool,
    pub(crate) started_at: u64,
    pub(crate) online_since: u64,
    pub(crate) restarts: AtomicU32,
    pub(crate) restart_pending: AtomicBool,
    pub(crate) restart: tokio::sync::Notify,
    pub(crate) relay_probe: Mutex<Option<Arc<dyn RelayProbe>>>,
}

fn is_invoice_id(s: &str) -> bool {
    s.len() == 32 && s.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// Characters allowed in a memo. Besides control characters this refuses the
/// invisible and direction-changing ones, which could be used to make a note
/// shown on the merchant's page read as something else.
pub fn is_plain_text(c: char) -> bool {
    !(c.is_control()
        || matches!(c,
            '\u{00AD}' | '\u{061C}' | '\u{180E}' | '\u{FEFF}'
            | '\u{200B}'..='\u{200F}'
            | '\u{2028}'..='\u{202F}'
            | '\u{2060}'..='\u{206F}'
            | '\u{E000}'..='\u{F8FF}'
            | '\u{FFF0}'..='\u{FFFF}'
            | '\u{E0000}'..='\u{E0FFF}'
            | '\u{F0000}'..='\u{10FFFF}'))
}

/// Derive the state of an invoice from what the chain shows.
pub fn evaluate(amount_sat: u64, received_sat: u64, confirmed_sat: u64, now: u64, expires_at: u64) -> Status {
    if confirmed_sat >= amount_sat {
        Status::Confirmed
    } else if received_sat >= amount_sat {
        Status::Detected
    } else if now > expires_at {
        Status::Expired
    } else {
        Status::Pending
    }
}

/// Total and sufficiently-confirmed satoshis among `payments`, capped at the
/// bitcoin supply so that a lying backend cannot produce unstorable numbers.
pub fn tally(payments: &[Payment], tip: u32, min_confirmations: u32) -> (u64, u64) {
    let mut received = 0u64;
    let mut confirmed = 0u64;
    for p in payments {
        received = received.saturating_add(p.value_sat);
        let deep_enough = p
            .height
            .filter(|h| *h <= tip)
            .is_some_and(|h| tip - h + 1 >= min_confirmations);
        if deep_enough {
            confirmed = confirmed.saturating_add(p.value_sat);
        }
    }
    (received.min(MAX_MONEY_SAT), confirmed.min(MAX_MONEY_SAT))
}

impl Service {
    /// Assemble the service from its parts. `server_pubkey` is the hex public
    /// key requests must be addressed to.
    ///
    /// Stored setting overrides are applied on top of the environment. If they
    /// no longer validate (for example because the environment changed), the
    /// environment values are used and the reason is reported in health.
    pub fn new(
        cfg: Config,
        db: Db,
        wallet: Wallet,
        chain: Arc<dyn Chain>,
        clock: Clock,
        server_pubkey: String,
    ) -> Result<Self> {
        let now = clock();
        let online_since = match db.meta_get(META_ONLINE_SINCE)?.and_then(|v| v.parse::<u64>().ok()) {
            Some(t) => t,
            None => {
                let t = db.first_created_at()?.map_or(now, |first| first.min(now));
                db.meta_set(META_ONLINE_SINCE, Some(&t.to_string()))?;
                t
            }
        };
        let extra: HashSet<String> = db.operators()?.into_iter().map(|o| o.pubkey).collect();
        let base = cfg.settings();
        let has_admins = !cfg.admins.is_empty() || !extra.is_empty();
        let (settings, problem) = match db.meta_get(META_SETTINGS)? {
            None => (base, None),
            Some(json) => match serde_json::from_str::<SettingsPatch>(&json) {
                Ok(patch) => {
                    let merged = patch.apply(&base);
                    let relays_ok = match &patch.relays {
                        Some(r) => crate::settings::check_relays(r, cfg.network),
                        None => Ok(()),
                    };
                    match relays_ok.and_then(|()| merged.validate(has_admins)) {
                        Ok(()) => (merged, None),
                        Err(e) => (base, Some(format!("saved settings were not applied: {e}"))),
                    }
                }
                Err(e) => (base, Some(format!("saved settings could not be read: {e}"))),
            },
        };
        if let Some(why) = &problem {
            tracing::warn!("{why}");
        }
        let paused = db.meta_get(META_PAUSED)?.is_some();
        let svc = Service {
            pow: PowGuard::new(settings.pow_bits, settings.pow_window_secs, POW_CACHE, server_pubkey),
            create_limiter: RateLimiter::new(settings.rate_global_per_min, settings.rate_per_key_per_min),
            request_limiter: RateLimiter::new(settings.rate_requests_per_min, settings.rate_requests_per_key_per_min),
            settings: RwLock::new(settings),
            settings_problem: Mutex::new(problem),
            extra_operators: RwLock::new(extra),
            paused: AtomicBool::new(paused),
            started_at: now,
            online_since,
            restarts: AtomicU32::new(0),
            restart_pending: AtomicBool::new(false),
            restart: tokio::sync::Notify::new(),
            relay_probe: Mutex::new(None),
            cfg,
            db,
            wallet,
            chain,
            clock,
            chain_state: Mutex::new(ChainState::default()),
        };
        Ok(svc)
    }

    /// The live settings.
    pub fn settings(&self) -> RwLockReadGuard<'_, Settings> {
        self.settings.read().unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Make `s` the live settings (it must already be validated).
    pub(crate) fn apply_settings(&self, s: Settings) {
        self.pow.set_policy(s.pow_bits, s.pow_window_secs);
        self.create_limiter
            .set_limits(s.rate_global_per_min, s.rate_per_key_per_min);
        self.request_limiter
            .set_limits(s.rate_requests_per_min, s.rate_requests_per_key_per_min);
        *self.settings.write().unwrap_or_else(std::sync::PoisonError::into_inner) = s;
    }

    /// Seconds between chain polls.
    pub fn poll_secs(&self) -> u64 {
        self.settings().poll_secs
    }

    /// Whether invoicing is stopped by an operator.
    pub fn is_paused(&self) -> bool {
        self.paused.load(Ordering::Relaxed)
    }

    /// Install the source of relay connection states.
    pub fn set_relay_probe(&self, probe: Option<Arc<dyn RelayProbe>>) {
        *self
            .relay_probe
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = probe;
    }

    /// Resolves when an operator (or a relay change) asks for a restart.
    pub async fn restart_requested(&self) {
        self.restart.notified().await;
    }

    /// Called by the supervisor once a restart has happened.
    pub fn restarted(&self) {
        self.restarts.fetch_add(1, Ordering::Relaxed);
        self.restart_pending.store(false, Ordering::Relaxed);
    }

    pub(crate) fn schedule_restart(&self) {
        self.restart_pending.store(true, Ordering::Relaxed);
        self.restart.notify_one();
    }

    /// First receive address, for the operator to compare with their wallet.
    pub fn first_address(&self) -> Result<String> {
        Ok(self.wallet.address(0)?.to_string())
    }

    fn is_admin(&self, caller: &Caller) -> bool {
        caller.pubkey.as_ref().is_some_and(|p| {
            self.cfg.admins.contains(p)
                || self
                    .extra_operators
                    .read()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .contains(p)
        })
    }

    pub(crate) fn require_admin(&self, caller: &Caller) -> Result<()> {
        if self.is_admin(caller) {
            Ok(())
        } else {
            Err(Error::Unauthorized)
        }
    }

    /// Every answer costs a signature and a publish to each relay, so calls by
    /// the public are budgeted. Operators are never throttled.
    pub(crate) fn admit(&self, caller: &Caller) -> Result<()> {
        if self.is_admin(caller) {
            return Ok(());
        }
        let key = caller.pubkey.as_deref().unwrap_or("");
        if self.request_limiter.allow(key, (self.clock)()) {
            Ok(())
        } else {
            Err(Error::RateLimited)
        }
    }

    fn view(&self, row: InvoiceRow) -> Invoice {
        Invoice {
            bip21: bip21(&row.address, row.amount_sat),
            id: row.id,
            address: row.address,
            amount_sat: row.amount_sat,
            memo: row.memo,
            status: row.status,
            received_sat: row.received_sat,
            confirmed_sat: row.confirmed_sat,
            created_at: row.created_at,
            expires_at: row.expires_at,
            settled_at: row.settled_at,
            confirmations_required: self.settings().min_confirmations,
        }
    }

    /// Take one unit of the public invoice-creation budget. Checked again here
    /// because another request may have taken the last unit since the cheap
    /// pre-check.
    fn spend_creation_budget(&self, admin: bool, pubkey: &str, now: u64) -> Result<()> {
        if admin || self.create_limiter.allow(pubkey, now) {
            Ok(())
        } else {
            Err(Error::RateLimited)
        }
    }

    /// Longest invoice lifetime the caller may ask for. The public cannot
    /// hold an invoice slot open longer than the default.
    fn expiry_limit(&self, admin: bool) -> u64 {
        let s = self.settings();
        if admin {
            s.max_expiry_secs
        } else {
            s.default_expiry_secs
        }
    }

    /// Public parameters of the service.
    pub fn info(&self, caller: &Caller) -> Result<Info> {
        self.admit(caller)?;
        let is_admin = self.is_admin(caller);
        let max_expiry_secs = self.expiry_limit(is_admin);
        let s = self.settings();
        Ok(Info {
            name: s.name.clone(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            network: self.cfg.network.to_string(),
            public_invoicing: s.public_invoicing,
            pow_bits: s.pow_bits,
            pow_window_secs: s.pow_window_secs,
            min_confirmations: s.min_confirmations,
            default_expiry_secs: s.default_expiry_secs,
            max_expiry_secs,
            min_amount_sat: s.min_amount_sat,
            max_amount_sat: s.max_amount_sat,
            is_admin,
        })
    }

    /// Create an invoice with a fresh address.
    ///
    /// Every creator must present a signed request event that is addressed to
    /// this service, fresh and unused. The public must also be allowed by
    /// configuration, find the service below its capacity limits, fit the rate
    /// limits and have mined NIP-13 work into that event. Cheap checks run
    /// first; nothing is consumed by a request that is going to be refused.
    pub fn create_invoice(&self, caller: &Caller, p: CreateInvoiceParams) -> Result<Invoice> {
        let pubkey = caller.pubkey.as_deref().ok_or(Error::Unauthorized)?;
        self.admit(caller)?;
        let admin = self.is_admin(caller);
        let now = (self.clock)();
        if self.is_paused() {
            return Err(Error::Paused);
        }
        let s = self.settings().clone();

        if p.amount_sat < s.min_amount_sat || p.amount_sat > s.max_amount_sat {
            return Err(Error::InvalidParams(format!(
                "amount_sat must be between {} and {}",
                s.min_amount_sat, s.max_amount_sat
            )));
        }
        let memo = p.memo.unwrap_or_default();
        if memo.chars().count() > MAX_MEMO_CHARS || !memo.chars().all(is_plain_text) {
            return Err(Error::InvalidParams(format!(
                "memo must be at most {MAX_MEMO_CHARS} visible characters"
            )));
        }
        let expiry = p.expiry_secs.unwrap_or(s.default_expiry_secs);
        let longest = self.expiry_limit(admin);
        if !(60..=longest).contains(&expiry) {
            return Err(Error::InvalidParams(format!(
                "expiry_secs must be between 60 and {longest}"
            )));
        }

        if !admin {
            if !s.public_invoicing {
                return Err(Error::Unauthorized);
            }
            // Open invoices cost polling; invoices per day cost address indices
            // (a wallet only looks a limited gap ahead of its last used one).
            if self.db.count_open()? >= s.max_open_invoices
                || self.db.count_created_since(now.saturating_sub(DAY_SECS))? >= s.max_invoices_per_day
            {
                return Err(Error::Capacity);
            }
            if !self.create_limiter.would_allow(pubkey, now) {
                return Err(Error::RateLimited);
            }
        }
        let event = caller
            .event
            .as_ref()
            .ok_or_else(|| Error::Pow("request event unavailable".into()))?;
        self.check_event(event, pubkey, now, admin)?;
        self.spend_creation_budget(admin, pubkey, now)?;

        let mut raw = [0u8; 16];
        // Without OS randomness invoice ids would be guessable; never continue.
        getrandom::getrandom(&mut raw).expect("operating system randomness is available");
        let id = hex::encode(raw);
        let row = self.db.insert(
            &NewInvoice {
                id: &id,
                amount_sat: p.amount_sat,
                memo: &memo,
                created_at: now,
                expires_at: now + expiry,
                creator: pubkey,
            },
            &|index| Ok(self.wallet.address(index)?.to_string()),
        )?;
        Ok(self.view(row))
    }

    /// Accept `event` as the one-time carrier of a state-changing request.
    /// Used ids are also stored in the database, so a captured request cannot
    /// be replayed after a restart either.
    pub(crate) fn check_event(&self, event: &Event, pubkey: &str, now: u64, exempt: bool) -> Result<()> {
        self.pow.check(event, pubkey, now, exempt)?;
        let forget_at = event.created_at.as_secs() + 2 * self.pow.window_secs();
        if !self.db.remember_request(&event.id.to_hex(), forget_at, now)? {
            return Err(Error::Pow("event already used".into()));
        }
        Ok(())
    }

    /// Fetch an invoice. The unguessable id is the capability; no login needed.
    pub fn get_invoice(&self, caller: &Caller, p: GetInvoiceParams) -> Result<Invoice> {
        self.admit(caller)?;
        if !is_invoice_id(&p.id) {
            return Err(Error::InvalidParams("id must be 32 lowercase hex characters".into()));
        }
        self.db.get(&p.id)?.map(|r| self.view(r)).ok_or(Error::NotFound)
    }

    /// Page through all invoices (operators only).
    pub fn list_invoices(&self, caller: &Caller, p: ListInvoicesParams) -> Result<InvoiceList> {
        self.admit(caller)?;
        self.require_admin(caller)?;
        let limit = p.limit.unwrap_or(50);
        if !(1..=MAX_PAGE).contains(&limit) {
            return Err(Error::InvalidParams(format!("limit must be between 1 and {MAX_PAGE}")));
        }
        let rows = self.db.list(p.status, p.before, limit)?;
        let next_before = if rows.len() as u32 == limit {
            rows.last().map(|r| r.index)
        } else {
            None
        };
        let invoices = rows
            .into_iter()
            .map(|r| AdminInvoice {
                index: r.index,
                creator: r.creator.clone(),
                invoice: self.view(r),
            })
            .collect();
        Ok(InvoiceList { invoices, next_before })
    }

    /// Totals and backend health (operators only).
    pub fn stats(&self, caller: &Caller) -> Result<Stats> {
        self.admit(caller)?;
        self.require_admin(caller)?;
        let t = self.db.totals()?;
        let h = *self
            .chain_state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        Ok(Stats {
            pending: t.pending,
            detected: t.detected,
            confirmed: t.confirmed,
            expired: t.expired,
            confirmed_sat: t.confirmed_sat,
            detected_sat: t.detected_sat,
            next_index: t.next_index,
            tip_height: h.tip,
            last_poll_at: h.last_poll_at,
            chain_ok: h.chain_ok,
            watch_errors: h.watch_errors,
        })
    }

    /// Refresh every watched invoice from the chain. Returns how many changed.
    ///
    /// One unreadable invoice, or an unreachable backend, never stops the
    /// others from being evaluated: whatever cannot be read keeps its last
    /// known amounts, and expiry is applied regardless.
    pub async fn poll_once(&self) -> Result<usize> {
        let report = self.poll_inner().await;
        let mut h = self
            .chain_state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        h.watch_errors = report.watch_errors;
        if report.tip.is_some() {
            h.tip = report.tip;
        }
        match report.failure {
            None => {
                h.chain_ok = true;
                h.last_poll_at = Some((self.clock)());
                Ok(report.changed)
            }
            Some(e) => {
                h.chain_ok = false;
                Err(e)
            }
        }
    }

    /// Ask the chain about one chunk of invoices.
    async fn look_up(&self, chunk: &[InvoiceRow]) -> Result<(u32, Vec<ScriptOutcome>)> {
        let scripts = chunk
            .iter()
            .map(|inv| self.wallet.script_for(&inv.address))
            .collect::<Result<Vec<_>>>()?;
        let snap = self.chain.snapshot(&scripts).await?;
        if snap.payments.len() != chunk.len() {
            return Err(Error::Chain("backend returned the wrong number of histories".into()));
        }
        Ok((snap.tip, snap.payments))
    }

    async fn poll_inner(&self) -> PollReport {
        let mut report = PollReport {
            changed: 0,
            tip: None,
            watch_errors: 0,
            failure: None,
        };
        let now = (self.clock)();
        let (late_grace_secs, min_confirmations) = {
            let s = self.settings();
            (s.late_grace_secs, s.min_confirmations)
        };
        let active = match self.db.active(now.saturating_sub(late_grace_secs)) {
            Ok(rows) => rows,
            Err(e) => {
                report.failure = Some(e);
                return report;
            }
        };
        // With nothing to watch, still ask for the tip: health then reports a
        // connection that was just used, and the current block height.
        if active.is_empty() {
            match self.chain.snapshot(&[]).await {
                Ok(snap) => report.tip = Some(snap.tip),
                Err(e) => report.failure = Some(e),
            }
            return report;
        }
        for chunk in active.chunks(POLL_CHUNK) {
            // After the first failure the backend is not asked again in this
            // round (each attempt could cost a full timeout).
            let seen = if report.failure.is_some() {
                None
            } else {
                match self.look_up(chunk).await {
                    Ok(found) => Some(found),
                    Err(e) => {
                        report.failure = Some(e);
                        None
                    }
                }
            };
            for (i, inv) in chunk.iter().enumerate() {
                let (received, confirmed) = match &seen {
                    Some((tip, outcomes)) => {
                        report.tip = Some(*tip);
                        match &outcomes[i] {
                            Ok(payments) => tally(payments, *tip, min_confirmations),
                            Err(why) => {
                                report.watch_errors += 1;
                                tracing::warn!(index = inv.index, %why, "cannot read payments for an invoice; it needs a look");
                                (inv.received_sat, inv.confirmed_sat)
                            }
                        }
                    }
                    None => (inv.received_sat, inv.confirmed_sat),
                };
                let status = evaluate(inv.amount_sat, received, confirmed, now, inv.expires_at);
                if (status, received, confirmed) != (inv.status, inv.received_sat, inv.confirmed_sat) {
                    let settled_at = (status == Status::Confirmed).then_some(now);
                    if let Err(e) = self.db.update(&inv.id, status, received, confirmed, settled_at) {
                        report.failure = Some(e);
                        return report;
                    }
                    // The id is a bearer capability, so only the index is logged.
                    tracing::info!(
                        index = inv.index,
                        from = inv.status.as_str(),
                        to = status.as_str(),
                        received,
                        confirmed,
                        "invoice updated"
                    );
                    report.changed += 1;
                }
            }
        }
        report
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::config::tests::{base, load};
    use crate::electrum::testutil::FakeElectrum;
    use crate::electrum::{Snapshot, MAX_HISTORY};
    use crate::pow::tests::{mined, SERVICE};
    use crate::pow::CTXVM_KIND;
    use crate::wallet::scripthash;
    use async_trait::async_trait;
    use bitcoin::ScriptBuf;
    use nostr_sdk::{EventBuilder, Keys, Kind, Tag, Timestamp};
    use std::sync::atomic::{AtomicU64, Ordering};

    pub struct Rig {
        pub svc: Arc<Service>,
        pub fake: FakeElectrum,
        pub now: Arc<AtomicU64>,
        pub admin_keys: Keys,
        seq: AtomicU64,
        /// Holds the data directory (backups) for the rig's lifetime.
        pub _dir: tempfile::TempDir,
    }

    pub const T0: u64 = 1_700_000_000;

    pub async fn rig(tweak: impl FnOnce(&mut Config)) -> Rig {
        let fake = FakeElectrum::start().await;
        let admin_keys = Keys::generate();
        let mut cfg = load(&base()).unwrap();
        cfg.admins = [admin_keys.public_key().to_hex()].into();
        cfg.public_invoicing = true;
        cfg.pow_bits = 4;
        cfg.min_confirmations = 2;
        let dir = tempfile::tempdir().unwrap();
        cfg.data_dir = dir.path().to_path_buf();
        tweak(&mut cfg);
        let now = Arc::new(AtomicU64::new(T0));
        let n = now.clone();
        let clock: Clock = Arc::new(move || n.load(Ordering::SeqCst));
        let wallet = Wallet::new(&cfg.xpub, cfg.network).unwrap();
        let chain = Arc::new(fake.client());
        let db = Db::open_in_memory().unwrap();
        let svc = Arc::new(Service::new(cfg, db, wallet, chain, clock, SERVICE.to_string()).unwrap());
        Rig {
            svc,
            fake,
            now,
            admin_keys,
            seq: AtomicU64::new(0),
            _dir: dir,
        }
    }

    /// A fresh, unmined request event from `keys`, addressed to the test service.
    pub fn request(keys: &Keys, created_at: u64, salt: u64) -> Event {
        EventBuilder::new(Kind::Custom(CTXVM_KIND), format!("{{\"n\":{salt}}}"))
            .tag(Tag::parse(["p", SERVICE]).unwrap())
            .custom_created_at(Timestamp::from(created_at))
            .sign_with_keys(keys)
            .unwrap()
    }

    /// A member of the public with freshly mined work.
    pub fn public(keys: &Keys, bits: u8, at: u64) -> Caller {
        Caller {
            pubkey: Some(keys.public_key().to_hex()),
            event: Some(mined(keys, bits, at)),
        }
    }

    fn params(amount_sat: u64) -> CreateInvoiceParams {
        CreateInvoiceParams {
            amount_sat,
            memo: None,
            expiry_secs: None,
        }
    }

    impl Rig {
        /// The operator, carrying a new signed request each time.
        pub fn admin(&self) -> Caller {
            let salt = self.seq.fetch_add(1, Ordering::SeqCst);
            let event = request(&self.admin_keys, self.now.load(Ordering::SeqCst), salt);
            Caller {
                pubkey: Some(self.admin_keys.public_key().to_hex()),
                event: Some(event),
            }
        }
        pub fn create(&self, amount_sat: u64) -> Invoice {
            self.svc.create_invoice(&self.admin(), params(amount_sat)).unwrap()
        }
        fn script(&self, inv: &Invoice) -> ScriptBuf {
            self.svc.wallet.script_for(&inv.address).unwrap()
        }
        fn get(&self, id: &str) -> Invoice {
            self.svc
                .get_invoice(&self.admin(), GetInvoiceParams { id: id.to_string() })
                .unwrap()
        }
        pub fn set_now(&self, t: u64) {
            self.now.store(t, Ordering::SeqCst);
        }
    }

    fn code<T: std::fmt::Debug>(r: Result<T>) -> &'static str {
        r.unwrap_err().code()
    }

    #[test]
    fn system_clock_is_current() {
        assert!(system_clock()() > T0);
    }

    #[test]
    fn evaluate_and_tally() {
        assert_eq!(evaluate(100, 0, 0, 5, 10), Status::Pending);
        assert_eq!(evaluate(100, 99, 0, 5, 10), Status::Pending);
        assert_eq!(evaluate(100, 99, 99, 11, 10), Status::Expired);
        assert_eq!(evaluate(100, 100, 0, 11, 10), Status::Detected);
        assert_eq!(evaluate(100, 150, 100, 11, 10), Status::Confirmed);

        let pays = [
            Payment {
                value_sat: 10,
                height: Some(100),
            },
            Payment {
                value_sat: 20,
                height: Some(99),
            },
            Payment {
                value_sat: 40,
                height: None,
            },
            // A height above the tip is a lie or a race; never counted as confirmed.
            Payment {
                value_sat: 80,
                height: Some(101),
            },
        ];
        assert_eq!(tally(&pays, 100, 1), (150, 30));
        assert_eq!(tally(&pays, 100, 2), (150, 20));
        assert_eq!(tally(&pays, 100, 3), (150, 0));
        // Absurd values are capped at the bitcoin supply instead of overflowing.
        let huge = [
            Payment {
                value_sat: u64::MAX,
                height: Some(1),
            },
            Payment {
                value_sat: 5,
                height: Some(1),
            },
        ];
        assert_eq!(tally(&huge, 100, 1), (MAX_MONEY_SAT, MAX_MONEY_SAT));
    }

    #[test]
    fn memo_character_rules() {
        for ok in ['a', 'é', '漢', '🙂', ' ', '-', '₿'] {
            assert!(is_plain_text(ok), "{ok:?}");
        }
        for bad in [
            '\n',
            '\u{7f}',
            '\u{200b}',
            '\u{202e}',
            '\u{2066}',
            '\u{feff}',
            '\u{e000}',
            '\u{ad}',
            '\u{e0041}',
            '\u{fffd}',
            '\u{61c}',
            '\u{180e}',
            '\u{2028}',
            '\u{f0000}',
            '\u{10ffff}',
        ] {
            assert!(!is_plain_text(bad), "{bad:?}");
        }
    }

    #[tokio::test]
    async fn info_reflects_config_and_caller() {
        let r = rig(|_| {}).await;
        let i = r.svc.info(&r.admin()).unwrap();
        assert!(i.is_admin && i.public_invoicing);
        assert_eq!(
            (i.name.as_str(), i.network.as_str(), i.pow_bits, i.min_confirmations),
            ("deadhop", "regtest", 4, 2)
        );
        assert_eq!(i.version, env!("CARGO_PKG_VERSION"));
        assert_eq!(i.max_expiry_secs, 86_400);
        let guest = r.svc.info(&Caller::default()).unwrap();
        assert!(!guest.is_admin);
        assert_eq!(
            guest.max_expiry_secs, 3_600,
            "the public cannot hold a slot longer than the default"
        );
        assert_eq!(r.svc.poll_secs(), 10);
        assert!(r.svc.first_address().unwrap().starts_with("bcrt1q"));
    }

    #[tokio::test]
    async fn operator_creates_invoices_without_work() {
        let r = rig(|c| c.public_invoicing = false).await;
        let p = CreateInvoiceParams {
            amount_sat: 50_000,
            memo: Some("order 7".into()),
            expiry_secs: Some(7_200),
        };
        let inv = r.svc.create_invoice(&r.admin(), p).unwrap();
        assert_eq!(inv.id.len(), 32);
        assert!(inv.address.starts_with("bcrt1q"));
        assert_eq!(inv.address, r.svc.first_address().unwrap());
        assert_eq!(inv.bip21, format!("bitcoin:{}?amount=0.00050000", inv.address));
        assert_eq!(
            (inv.status, inv.memo.as_str(), inv.created_at, inv.expires_at),
            (Status::Pending, "order 7", T0, T0 + 7_200)
        );
        assert_eq!(inv.confirmations_required, 2);
        assert_eq!(r.get(&inv.id), inv);

        let second = r.create(50_000);
        assert_ne!(second.address, inv.address, "addresses are never reused");
        assert_ne!(second.id, inv.id);
        assert_eq!(second.expires_at, T0 + 3_600);
    }

    #[tokio::test]
    async fn operator_requests_cannot_be_replayed_or_borrowed() {
        let r = rig(|_| {}).await;
        let me = Some(r.admin_keys.public_key().to_hex());

        // The same signed request presented twice (e.g. re-wrapped by whoever captured it).
        let once = r.admin();
        r.svc.create_invoice(&once, params(5_000)).unwrap();
        assert_eq!(code(r.svc.create_invoice(&once, params(5_000))), "pow_rejected");

        // A request the operator signed long ago.
        let stale = Caller {
            pubkey: me.clone(),
            event: Some(request(&r.admin_keys, T0 - 5_000, 1)),
        };
        assert_eq!(code(r.svc.create_invoice(&stale, params(5_000))), "pow_rejected");

        // A request the operator signed for a different service.
        let elsewhere = EventBuilder::new(Kind::Custom(CTXVM_KIND), "{}")
            .tag(Tag::parse(["p", &"ab".repeat(32)]).unwrap())
            .custom_created_at(Timestamp::from(T0))
            .sign_with_keys(&r.admin_keys)
            .unwrap();
        let borrowed = Caller {
            pubkey: me.clone(),
            event: Some(elsewhere),
        };
        assert_eq!(code(r.svc.create_invoice(&borrowed, params(5_000))), "pow_rejected");

        // No signed event at all (e.g. a reassembled oversized request).
        assert_eq!(
            code(r.svc.create_invoice(
                &Caller {
                    pubkey: me,
                    event: None
                },
                params(5_000)
            )),
            "pow_rejected"
        );
        assert_eq!(r.svc.stats(&r.admin()).unwrap().next_index, 1);
    }

    #[tokio::test]
    async fn create_invoice_validates_parameters() {
        let r = rig(|_| {}).await;
        let bad = |p: CreateInvoiceParams| {
            let e = r.svc.create_invoice(&r.admin(), p).unwrap_err();
            assert_eq!(e.code(), "invalid_params");
            e.to_string()
        };
        assert!(bad(params(999)).contains("amount_sat"));
        assert!(bad(params(100_000_001)).contains("amount_sat"));
        assert!(bad(CreateInvoiceParams {
            memo: Some("x".repeat(201)),
            ..params(5_000)
        })
        .contains("memo"));
        assert!(bad(CreateInvoiceParams {
            memo: Some("a\nb".into()),
            ..params(5_000)
        })
        .contains("memo"));
        // A right-to-left override could make a note read as something else on the pay page.
        assert!(bad(CreateInvoiceParams {
            memo: Some("pay \u{202e}1cb".into()),
            ..params(5_000)
        })
        .contains("memo"));
        assert!(bad(CreateInvoiceParams {
            expiry_secs: Some(59),
            ..params(5_000)
        })
        .contains("between 60 and 86400"));
        assert!(bad(CreateInvoiceParams {
            expiry_secs: Some(86_401),
            ..params(5_000)
        })
        .contains("expiry_secs"));
        // 200 multi-byte characters are within the limit.
        let long = CreateInvoiceParams {
            memo: Some("é".repeat(200)),
            ..params(5_000)
        };
        r.svc.create_invoice(&r.admin(), long).unwrap();
        assert_eq!(
            code(r.svc.create_invoice(&Caller::default(), params(5_000))),
            "unauthorized"
        );
    }

    #[tokio::test]
    async fn public_callers_need_permission_work_and_budget() {
        let keys = Keys::generate();

        // Public invoicing disabled.
        let r = rig(|c| c.public_invoicing = false).await;
        assert_eq!(
            code(r.svc.create_invoice(&public(&keys, 4, T0), params(5_000))),
            "unauthorized"
        );

        let r = rig(|c| c.rate_per_key_per_min = 2).await;
        // No event (e.g. reassembled oversized request).
        let bare = Caller {
            pubkey: Some(keys.public_key().to_hex()),
            event: None,
        };
        assert_eq!(code(r.svc.create_invoice(&bare, params(5_000))), "pow_rejected");
        // Stale work, and no work at all.
        assert_eq!(
            code(r.svc.create_invoice(&public(&keys, 4, T0 - 5_000), params(5_000))),
            "pow_rejected"
        );
        let unmined = Caller {
            pubkey: Some(keys.public_key().to_hex()),
            event: Some(request(&keys, T0, 0)),
        };
        let e = r.svc.create_invoice(&unmined, params(5_000));
        assert!(
            e.unwrap_err().to_string().contains("work"),
            "an event without a nonce tag is refused"
        );
        // Valid work, once.
        let caller = public(&keys, 4, T0);
        let inv = r.svc.create_invoice(&caller, params(5_000)).unwrap();
        assert_eq!(inv.status, Status::Pending);
        assert_eq!(code(r.svc.create_invoice(&caller, params(5_000))), "pow_rejected");
        // The public may not ask for a longer lifetime than the default.
        let long = CreateInvoiceParams {
            expiry_secs: Some(3_601),
            ..params(5_000)
        };
        let e = r.svc.create_invoice(&public(&keys, 4, T0 + 9), long).unwrap_err();
        assert!(e.to_string().contains("between 60 and 3600"), "{e}");
        // Second fresh request is fine, third exceeds the per-key budget ...
        r.svc.create_invoice(&public(&keys, 4, T0 + 1), params(5_000)).unwrap();
        let third = public(&keys, 4, T0 + 2);
        assert_eq!(code(r.svc.create_invoice(&third, params(5_000))), "rate_limited");
        // ... without using up the work: the same event is accepted a minute later.
        r.set_now(T0 + 60);
        r.svc.create_invoice(&third, params(5_000)).unwrap();
    }

    #[tokio::test]
    async fn creation_budget_is_enforced_at_commit_time() {
        let r = rig(|c| c.rate_global_per_min = 1).await;
        let (a, b) = (Keys::generate(), Keys::generate());
        r.svc.create_invoice(&public(&a, 4, T0), params(5_000)).unwrap();
        // Refused by the cheap pre-check ...
        assert_eq!(
            code(r.svc.create_invoice(&public(&b, 4, T0), params(5_000))),
            "rate_limited"
        );
        // ... and by the commit-time check, which is what holds when two
        // requests pass the pre-check together and only one unit is left.
        assert_eq!(code(r.svc.spend_creation_budget(false, "someone", T0)), "rate_limited");
        r.svc.spend_creation_budget(true, "operator", T0).unwrap();
        r.svc.spend_creation_budget(false, "someone", T0 + 60).unwrap();
    }

    #[tokio::test]
    async fn open_and_daily_caps_apply_to_the_public_only() {
        let r = rig(|c| c.max_open_invoices = 1).await;
        let keys = Keys::generate();
        r.svc.create_invoice(&public(&keys, 4, T0), params(5_000)).unwrap();
        assert_eq!(
            code(r.svc.create_invoice(&public(&keys, 4, T0 + 1), params(5_000))),
            "capacity"
        );
        r.create(5_000);
        // Once the open invoices expire there is room for the public again.
        r.set_now(T0 + 7_200);
        r.svc.poll_once().await.unwrap();
        r.svc
            .create_invoice(&public(&keys, 4, T0 + 7_200), params(5_000))
            .unwrap();

        // The daily cap bounds how many address indices can be used up, however short-lived the invoices.
        let r = rig(|c| c.max_invoices_per_day = 2).await;
        r.svc.create_invoice(&public(&keys, 4, T0), params(5_000)).unwrap();
        r.create(5_000);
        assert_eq!(
            code(r.svc.create_invoice(&public(&keys, 4, T0 + 61), params(5_000))),
            "capacity"
        );
        r.create(5_000);
        r.set_now(T0 + DAY_SECS + 1);
        r.svc
            .create_invoice(&public(&keys, 4, T0 + DAY_SECS + 1), params(5_000))
            .unwrap();
    }

    #[tokio::test]
    async fn every_tool_is_budgeted_for_the_public() {
        let r = rig(|c| c.rate_requests_per_key_per_min = 3).await;
        let keys = Keys::generate();
        let me = Caller {
            pubkey: Some(keys.public_key().to_hex()),
            event: None,
        };
        let missing = || GetInvoiceParams { id: "0".repeat(32) };
        r.svc.info(&me).unwrap();
        assert_eq!(code(r.svc.get_invoice(&me, missing())), "not_found");
        assert_eq!(code(r.svc.stats(&me)), "unauthorized");
        // Budget spent: every tool now answers with the same refusal.
        assert_eq!(code(r.svc.info(&me)), "rate_limited");
        assert_eq!(code(r.svc.get_invoice(&me, missing())), "rate_limited");
        assert_eq!(
            code(r.svc.list_invoices(&me, ListInvoicesParams::default())),
            "rate_limited"
        );
        assert_eq!(code(r.svc.stats(&me)), "rate_limited");
        assert_eq!(
            code(r.svc.create_invoice(&public(&keys, 4, T0), params(5_000))),
            "rate_limited"
        );
        // Other callers and the operator are unaffected; callers without a key share one bucket.
        r.svc.info(&Caller::default()).unwrap();
        for _ in 0..10 {
            r.svc.info(&r.admin()).unwrap();
        }
        r.set_now(T0 + 60);
        r.svc.info(&me).unwrap();
    }

    #[tokio::test]
    async fn get_invoice_checks_id_shape() {
        let r = rig(|_| {}).await;
        for id in [
            "",
            "abc",
            &"g".repeat(32),
            &"A".repeat(32),
            &"a".repeat(33),
            "' OR 1=1 --",
        ] {
            let e = r
                .svc
                .get_invoice(&r.admin(), GetInvoiceParams { id: id.to_string() })
                .unwrap_err();
            assert_eq!(e.code(), "invalid_params", "{id}");
        }
        assert_eq!(
            code(r.svc.get_invoice(&r.admin(), GetInvoiceParams { id: "0".repeat(32) })),
            "not_found"
        );
    }

    #[tokio::test]
    async fn listing_and_stats_are_operator_only() {
        let r = rig(|_| {}).await;
        let keys = Keys::generate();
        let stranger = public(&keys, 4, T0);
        assert_eq!(
            code(r.svc.list_invoices(&stranger, ListInvoicesParams::default())),
            "unauthorized"
        );
        assert_eq!(code(r.svc.stats(&stranger)), "unauthorized");
        assert_eq!(code(r.svc.stats(&Caller::default())), "unauthorized");

        let ids: Vec<String> = (0..3).map(|_| r.create(5_000).id).collect();
        let page = r
            .svc
            .list_invoices(
                &r.admin(),
                ListInvoicesParams {
                    limit: Some(2),
                    ..Default::default()
                },
            )
            .unwrap();
        assert_eq!(
            page.invoices.iter().map(|i| i.invoice.id.clone()).collect::<Vec<_>>(),
            [ids[2].clone(), ids[1].clone()]
        );
        assert_eq!(
            (page.invoices[0].index, page.invoices[0].creator.clone()),
            (2, r.admin_keys.public_key().to_hex())
        );
        assert_eq!(page.next_before, Some(1));
        let next = ListInvoicesParams {
            limit: Some(2),
            before: page.next_before,
            status: None,
        };
        let rest = r.svc.list_invoices(&r.admin(), next).unwrap();
        assert_eq!(rest.invoices.len(), 1);
        assert_eq!(rest.next_before, None);
        let confirmed = ListInvoicesParams {
            status: Some(Status::Confirmed),
            ..Default::default()
        };
        assert!(r.svc.list_invoices(&r.admin(), confirmed).unwrap().invoices.is_empty());
        for limit in [0, 201] {
            let e = r.svc.list_invoices(
                &r.admin(),
                ListInvoicesParams {
                    limit: Some(limit),
                    ..Default::default()
                },
            );
            assert_eq!(code(e), "invalid_params");
        }
        // The flattened admin view serialises to one flat object.
        let json = serde_json::to_value(&page.invoices[0]).unwrap();
        assert_eq!(json["index"], 2);
        assert_eq!(json["id"], ids[2]);

        let s = r.svc.stats(&r.admin()).unwrap();
        assert_eq!(
            (
                s.pending,
                s.next_index,
                s.tip_height,
                s.last_poll_at,
                s.chain_ok,
                s.watch_errors
            ),
            (3, 3, None, None, false, 0)
        );
    }

    #[tokio::test]
    async fn payment_lifecycle() {
        let r = rig(|_| {}).await;
        let inv = r.create(10_000);
        let script = r.script(&inv);
        assert_eq!(r.svc.poll_once().await.unwrap(), 0);

        // Partial payment in the mempool.
        let t1 = r.fake.pay(&script, 4_000, 0);
        assert_eq!(r.svc.poll_once().await.unwrap(), 1);
        let v = r.get(&inv.id);
        assert_eq!((v.status, v.received_sat, v.confirmed_sat), (Status::Pending, 4_000, 0));

        // Remainder arrives: fully seen but unconfirmed.
        let t2 = r.fake.pay(&script, 6_000, 0);
        r.svc.poll_once().await.unwrap();
        assert_eq!(r.get(&inv.id).status, Status::Detected);

        // One confirmation is not enough when two are required.
        r.fake.set(|s| s.tip = 101);
        r.fake.set_height(&script, &t1, 101);
        r.fake.set_height(&script, &t2, 101);
        r.svc.poll_once().await.unwrap();
        let v = r.get(&inv.id);
        assert_eq!((v.status, v.confirmed_sat, v.settled_at), (Status::Detected, 0, None));

        // Second confirmation settles it.
        r.fake.set(|s| s.tip = 102);
        r.set_now(T0 + 900);
        assert_eq!(r.svc.poll_once().await.unwrap(), 1);
        let v = r.get(&inv.id);
        assert_eq!(
            (v.status, v.received_sat, v.confirmed_sat, v.settled_at),
            (Status::Confirmed, 10_000, 10_000, Some(T0 + 900))
        );

        // Settled invoices are no longer polled; only the tip is asked for.
        let before = r.fake.set(|s| s.calls.len());
        r.fake.set(|s| s.tip = 103);
        assert_eq!(r.svc.poll_once().await.unwrap(), 0);
        assert_eq!(
            r.fake.set(|s| s.calls[before..].to_vec()),
            ["blockchain.headers.subscribe"]
        );

        let s = r.svc.stats(&r.admin()).unwrap();
        assert_eq!(
            (s.confirmed, s.confirmed_sat, s.tip_height, s.last_poll_at, s.chain_ok),
            (1, 10_000, Some(103), Some(T0 + 900), true)
        );
    }

    #[tokio::test]
    async fn with_nothing_to_watch_the_poll_still_checks_the_chain() {
        let r = rig(|_| {}).await;
        r.fake.set(|s| s.tip = 840_000);
        assert_eq!(r.svc.poll_once().await.unwrap(), 0);
        let s = r.svc.stats(&r.admin()).unwrap();
        assert_eq!((s.tip_height, s.chain_ok), (Some(840_000), true));

        // An unreachable backend shows as such even with no invoices; the
        // last known height is kept.
        r.fake.set(|s| {
            s.error_on.insert("blockchain.headers.subscribe".into());
        });
        assert!(r.svc.poll_once().await.is_err());
        let s = r.svc.stats(&r.admin()).unwrap();
        assert_eq!((s.tip_height, s.chain_ok), (Some(840_000), false));
    }

    #[tokio::test]
    async fn unconfirmed_payment_that_disappears_is_rolled_back() {
        let r = rig(|_| {}).await;
        let inv = r.create(10_000);
        let script = r.script(&inv);
        r.fake.pay(&script, 10_000, 0);
        r.svc.poll_once().await.unwrap();
        assert_eq!(r.get(&inv.id).status, Status::Detected);
        // Replaced or evicted from the mempool (double spend).
        r.fake.set(|s| s.history.clear());
        r.svc.poll_once().await.unwrap();
        let v = r.get(&inv.id);
        assert_eq!((v.status, v.received_sat), (Status::Pending, 0));
    }

    #[tokio::test]
    async fn expiry_and_late_payment_grace() {
        let r = rig(|c| c.late_grace_secs = 1_000).await;
        let short = || CreateInvoiceParams {
            expiry_secs: Some(60),
            ..params(10_000)
        };
        let inv = r.svc.create_invoice(&r.admin(), short()).unwrap();
        let gone = r.svc.create_invoice(&r.admin(), short()).unwrap();
        r.set_now(T0 + 61);
        assert_eq!(r.svc.poll_once().await.unwrap(), 2);
        assert_eq!(r.get(&inv.id).status, Status::Expired);

        // Paid late but within the grace period: still credited.
        r.fake.pay(&r.script(&inv), 10_000, 100);
        r.fake.set(|s| s.tip = 101);
        r.svc.poll_once().await.unwrap();
        assert_eq!(r.get(&inv.id).status, Status::Confirmed);

        // After the grace period the other invoice is no longer watched.
        r.set_now(T0 + 60 + 1_001);
        r.fake.pay(&r.script(&gone), 10_000, 100);
        assert_eq!(r.svc.poll_once().await.unwrap(), 0);
        assert_eq!(r.get(&gone.id).status, Status::Expired);
    }

    #[tokio::test]
    async fn polls_in_chunks() {
        let r = rig(|_| {}).await;
        let invs: Vec<Invoice> = (0..POLL_CHUNK + 5).map(|_| r.create(5_000)).collect();
        r.fake.pay(&r.script(&invs[0]), 5_000, 0);
        r.fake.pay(&r.script(&invs[POLL_CHUNK + 4]), 5_000, 0);
        assert_eq!(r.svc.poll_once().await.unwrap(), 2);
        assert_eq!(
            r.fake
                .set(|s| s.calls.iter().filter(|m| *m == "blockchain.headers.subscribe").count()),
            2
        );
    }

    /// The attack: flood one invoice's address with dust so its history is too
    /// long to read. Every other invoice must keep working.
    #[tokio::test]
    async fn one_unreadable_invoice_does_not_stall_the_others() {
        let r = rig(|_| {}).await;
        let poisoned = r.create(10_000);
        let healthy = r.create(10_000);
        let spam: Vec<(String, i64)> = (0..=MAX_HISTORY).map(|i| (format!("{i:064x}"), 10)).collect();
        r.fake.set(|s| s.history.insert(scripthash(&r.script(&poisoned)), spam));
        r.fake.pay(&r.script(&healthy), 10_000, 99);

        assert_eq!(
            r.svc.poll_once().await.unwrap(),
            1,
            "the poll succeeds and the healthy invoice moves"
        );
        assert_eq!(r.get(&healthy.id).status, Status::Confirmed);
        assert_eq!(r.get(&poisoned.id).status, Status::Pending);
        let s = r.svc.stats(&r.admin()).unwrap();
        assert_eq!((s.chain_ok, s.watch_errors), (true, 1));

        // The unreadable invoice still expires, and eventually stops being watched.
        r.set_now(T0 + 3_601);
        r.svc.poll_once().await.unwrap();
        assert_eq!(r.get(&poisoned.id).status, Status::Expired);
        r.set_now(T0 + 3_600 + 86_400 + 1);
        r.svc.poll_once().await.unwrap();
        assert_eq!(r.svc.stats(&r.admin()).unwrap().watch_errors, 0);
    }

    #[tokio::test]
    async fn chain_failure_is_reported_and_recovers_while_expiry_continues() {
        let r = rig(|_| {}).await;
        let invs: Vec<Invoice> = (0..POLL_CHUNK + 1).map(|_| r.create(5_000)).collect();
        r.fake.set(|s| s.error_on.insert("blockchain.headers.subscribe".into()));
        assert_eq!(code(r.svc.poll_once().await), "internal");
        let s = r.svc.stats(&r.admin()).unwrap();
        assert_eq!((s.chain_ok, s.last_poll_at), (false, None));
        // The backend was asked once, not once per chunk.
        assert_eq!(
            r.fake
                .set(|s| s.calls.iter().filter(|m| *m == "blockchain.headers.subscribe").count()),
            1
        );

        // Still down an hour later: invoices in every chunk expire anyway.
        r.set_now(T0 + 3_601);
        assert!(r.svc.poll_once().await.is_err());
        assert_eq!(r.get(&invs[0].id).status, Status::Expired);
        assert_eq!(r.get(&invs[POLL_CHUNK].id).status, Status::Expired);

        r.fake.set(|s| s.error_on.clear());
        r.svc.poll_once().await.unwrap();
        assert!(r.svc.stats(&r.admin()).unwrap().chain_ok);
    }

    #[tokio::test]
    async fn storage_failure_is_an_internal_error() {
        let r = rig(|_| {}).await;
        let inv = r.create(5_000);
        r.fake.pay(&r.script(&inv), 5_000, 0);
        // The poll reads fine but cannot write the update.
        r.svc.db.execute_raw(
            "CREATE TRIGGER no_updates BEFORE UPDATE ON invoices BEGIN SELECT RAISE(ABORT, 'read only'); END",
        );
        assert_eq!(code(r.svc.poll_once().await), "internal");
        assert!(!r.svc.stats(&r.admin()).unwrap().chain_ok);

        r.svc.db.execute_raw("DROP TABLE invoices");
        let keys = Keys::generate();
        for caller in [r.admin(), public(&keys, 4, T0)] {
            let e = r.svc.create_invoice(&caller, params(5_000)).unwrap_err();
            assert_eq!((e.code(), e.public_message().as_str()), ("internal", "internal error"));
        }
        assert!(r
            .svc
            .get_invoice(&r.admin(), GetInvoiceParams { id: "0".repeat(32) })
            .is_err());
        assert!(r.svc.list_invoices(&r.admin(), ListInvoicesParams::default()).is_err());
        assert!(r.svc.stats(&r.admin()).is_err());
        assert!(r.svc.poll_once().await.is_err());
    }

    #[tokio::test]
    async fn health_survives_a_poisoned_lock() {
        let r = rig(|_| {}).await;
        let svc = r.svc.clone();
        let _ = std::thread::spawn(move || {
            let _held = svc.chain_state.lock().unwrap();
            panic!("poison");
        })
        .join();
        r.svc.poll_once().await.unwrap();
        assert!(r.svc.stats(&r.admin()).unwrap().chain_ok);
    }

    struct ShortChain;
    #[async_trait]
    impl Chain for ShortChain {
        async fn genesis_hash(&self) -> Result<String> {
            Ok(String::new())
        }
        async fn snapshot(&self, _: &[ScriptBuf]) -> Result<Snapshot> {
            Ok(Snapshot {
                tip: 1,
                payments: vec![],
            })
        }
    }

    #[tokio::test]
    async fn misbehaving_backend_and_corrupt_rows_are_errors() {
        let r = rig(|_| {}).await;
        let wallet = Wallet::new(&r.svc.cfg.xpub, r.svc.cfg.network).unwrap();
        let db = Db::open_in_memory().unwrap();
        let svc = Service::new(
            r.svc.cfg.clone(),
            db,
            wallet,
            Arc::new(ShortChain),
            system_clock(),
            SERVICE.to_string(),
        )
        .unwrap();
        assert_eq!(ShortChain.genesis_hash().await.unwrap(), "");
        assert_eq!(svc.poll_once().await.unwrap(), 0, "nothing to watch");
        let now = system_clock()();
        let fresh = Caller {
            pubkey: Some(r.admin_keys.public_key().to_hex()),
            event: Some(request(&r.admin_keys, now, 0)),
        };
        svc.create_invoice(&fresh, params(5_000)).unwrap();
        let e = svc.poll_once().await.unwrap_err().to_string();
        assert!(e.contains("wrong number of histories"), "{e}");

        // An address that does not parse for this network is refused, not skipped.
        let broken = NewInvoice {
            id: &"1".repeat(32),
            amount_sat: 1,
            memo: "",
            created_at: T0,
            expires_at: T0 + 60,
            creator: "x",
        };
        r.svc.db.insert(&broken, &|_| Ok("not-an-address".into())).unwrap();
        assert!(r.svc.poll_once().await.is_err());
    }
}
