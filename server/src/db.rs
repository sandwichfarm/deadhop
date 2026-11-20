//! SQLite persistence. Every statement is parameterised.
//!
//! Besides invoices the database keeps small bookkeeping tables: `meta` (the
//! address high-water mark, first start, flags, settings overrides), the ids
//! of used request events (replay protection that survives restarts), extra
//! operator keys added from the dashboard, and an audit log of operator actions.

use std::path::Path;
use std::sync::Mutex;

use rmcp::schemars::{self, JsonSchema};
use rusqlite::{params, Connection, OptionalExtension, Row};
use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

/// Invoice lifecycle state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    /// Waiting for payment.
    Pending,
    /// Full amount seen, not yet sufficiently confirmed.
    Detected,
    /// Full amount confirmed. Terminal.
    Confirmed,
    /// Lifetime ended without full payment.
    Expired,
}

impl Status {
    /// Database / wire spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            Status::Pending => "pending",
            Status::Detected => "detected",
            Status::Confirmed => "confirmed",
            Status::Expired => "expired",
        }
    }

    fn parse(s: &str) -> Result<Status> {
        match s {
            "pending" => Ok(Status::Pending),
            "detected" => Ok(Status::Detected),
            "confirmed" => Ok(Status::Confirmed),
            "expired" => Ok(Status::Expired),
            other => Err(Error::Chain(format!("corrupt status '{other}' in database"))),
        }
    }
}

/// A stored invoice.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InvoiceRow {
    pub id: String,
    pub index: u32,
    pub address: String,
    pub amount_sat: u64,
    pub memo: String,
    pub status: Status,
    pub received_sat: u64,
    pub confirmed_sat: u64,
    pub created_at: u64,
    pub expires_at: u64,
    pub settled_at: Option<u64>,
    pub creator: String,
}

/// Fields supplied when creating an invoice.
pub struct NewInvoice<'a> {
    pub id: &'a str,
    pub amount_sat: u64,
    pub memo: &'a str,
    pub created_at: u64,
    pub expires_at: u64,
    pub creator: &'a str,
}

/// Aggregate figures for the dashboard.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Totals {
    pub pending: u64,
    pub detected: u64,
    pub confirmed: u64,
    pub expired: u64,
    pub confirmed_sat: u64,
    pub detected_sat: u64,
    pub next_index: u32,
}

const COLUMNS: &str = "id, idx, address, amount_sat, memo, status, received_sat, confirmed_sat, created_at, expires_at, settled_at, creator";

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS invoices (
    id            TEXT PRIMARY KEY,
    idx           INTEGER NOT NULL UNIQUE,
    address       TEXT NOT NULL UNIQUE,
    amount_sat    INTEGER NOT NULL,
    memo          TEXT NOT NULL,
    status        TEXT NOT NULL,
    received_sat  INTEGER NOT NULL DEFAULT 0,
    confirmed_sat INTEGER NOT NULL DEFAULT 0,
    created_at    INTEGER NOT NULL,
    expires_at    INTEGER NOT NULL,
    settled_at    INTEGER,
    creator       TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS invoices_status ON invoices(status);
CREATE INDEX IF NOT EXISTS invoices_settled ON invoices(settled_at);
CREATE TABLE IF NOT EXISTS meta (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS used_requests (
    id        TEXT PRIMARY KEY,
    forget_at INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS operators (
    pubkey   TEXT PRIMARY KEY,
    label    TEXT NOT NULL,
    added_at INTEGER NOT NULL,
    added_by TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS audit (
    id     INTEGER PRIMARY KEY AUTOINCREMENT,
    at     INTEGER NOT NULL,
    actor  TEXT NOT NULL,
    action TEXT NOT NULL,
    detail TEXT NOT NULL
);
";

/// `meta` key of the next unused derivation index. Never decreases, so a
/// wiped database can never hand out an address that was already used.
const NEXT_INDEX: &str = "next_index";
/// Identifier of the account key this database belongs to.
const ACCOUNT: &str = "account_id";
const UPSERT_META: &str =
    "INSERT INTO meta (key, value) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET value = excluded.value";
const REMEMBER_REQUEST: &str = "INSERT OR IGNORE INTO used_requests (id, forget_at) VALUES (?1, ?2)";
const BACKFILL_NEXT_INDEX: &str = "INSERT OR IGNORE INTO meta (key, value)
     SELECT ?1, CAST(COALESCE(MAX(idx) + 1, 0) AS TEXT) FROM invoices";

/// A stored audit entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuditRow {
    pub at: u64,
    pub actor: String,
    pub action: String,
    pub detail: String,
}

/// An operator key added from the dashboard.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OperatorRow {
    pub pubkey: String,
    pub label: String,
    pub added_at: u64,
    pub added_by: String,
}

/// Result of an integrity check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DbCheck {
    pub ok: bool,
    pub size_bytes: u64,
    pub invoices: u64,
}

fn row_to_invoice(r: &Row<'_>) -> rusqlite::Result<(InvoiceRow, String)> {
    let status: String = r.get(5)?;
    Ok((
        InvoiceRow {
            id: r.get(0)?,
            index: r.get(1)?,
            address: r.get(2)?,
            amount_sat: r.get(3)?,
            memo: r.get(4)?,
            status: Status::Pending,
            received_sat: r.get(6)?,
            confirmed_sat: r.get(7)?,
            created_at: r.get(8)?,
            expires_at: r.get(9)?,
            settled_at: r.get(10)?,
            creator: r.get(11)?,
        },
        status,
    ))
}

/// One `GROUP BY status` row: status, count, confirmed sum, received sum.
fn totals_row(r: &Row<'_>) -> rusqlite::Result<(String, u64, u64, u64)> {
    Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
}

fn audit_row(r: &Row<'_>) -> rusqlite::Result<AuditRow> {
    Ok(AuditRow {
        at: r.get(0)?,
        actor: r.get(1)?,
        action: r.get(2)?,
        detail: r.get(3)?,
    })
}

fn operator_row(r: &Row<'_>) -> rusqlite::Result<OperatorRow> {
    Ok(OperatorRow {
        pubkey: r.get(0)?,
        label: r.get(1)?,
        added_at: r.get(2)?,
        added_by: r.get(3)?,
    })
}

fn bucket_row(r: &Row<'_>) -> rusqlite::Result<(u64, u64, u64)> {
    Ok((r.get(0)?, r.get(1)?, r.get(2)?))
}

fn finish(pair: (InvoiceRow, String)) -> Result<InvoiceRow> {
    let (mut row, status) = pair;
    row.status = Status::parse(&status)?;
    Ok(row)
}

/// Handle to the invoice database.
pub struct Db {
    conn: Mutex<Connection>,
}

impl Db {
    /// Open (and create if needed) the database file, private to the owner.
    /// SQLite gives its journal files the same permissions as the database.
    pub fn open(path: &Path) -> Result<Db> {
        use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
        let file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .mode(0o600)
            .open(path)?;
        file.set_permissions(std::fs::Permissions::from_mode(0o600))?;
        drop(file);
        Self::init(Connection::open(path)?)
    }

    /// Private in-memory database (tests, dry runs).
    pub fn open_in_memory() -> Result<Db> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(conn: Connection) -> Result<Db> {
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL; PRAGMA foreign_keys=ON;")?;
        conn.execute_batch(SCHEMA)?;
        // Databases from before the high-water mark existed: start it above
        // every index already used.
        conn.execute(BACKFILL_NEXT_INDEX, [NEXT_INDEX])?;
        Ok(Db { conn: Mutex::new(conn) })
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Connection> {
        // A poisoned lock only means another thread panicked mid-statement;
        // SQLite's own transaction rollback keeps the data consistent.
        self.conn.lock().unwrap_or_else(|p| p.into_inner())
    }

    /// Allocate the next derivation index and store a new invoice atomically.
    /// `derive` turns the index into the receiving address.
    pub fn insert(&self, new: &NewInvoice<'_>, derive: &dyn Fn(u32) -> Result<String>) -> Result<InvoiceRow> {
        let mut conn = self.lock();
        let tx = conn.transaction()?;
        let index: u32 = tx.query_row(
            "SELECT MAX(COALESCE((SELECT MAX(idx) + 1 FROM invoices), 0),
                        COALESCE((SELECT CAST(value AS INTEGER) FROM meta WHERE key = ?1), 0))",
            [NEXT_INDEX],
            |r| r.get(0),
        )?;
        let address = derive(index)?;
        let next = (u64::from(index) + 1).to_string();
        tx.execute(UPSERT_META, params![NEXT_INDEX, next])?;
        tx.execute(
            "INSERT INTO invoices (id, idx, address, amount_sat, memo, status, created_at, expires_at, creator)
             VALUES (?1, ?2, ?3, ?4, ?5, 'pending', ?6, ?7, ?8)",
            params![
                new.id,
                index,
                address,
                new.amount_sat,
                new.memo,
                new.created_at,
                new.expires_at,
                new.creator
            ],
        )?;
        tx.commit()?;
        Ok(InvoiceRow {
            id: new.id.to_string(),
            index,
            address,
            amount_sat: new.amount_sat,
            memo: new.memo.to_string(),
            status: Status::Pending,
            received_sat: 0,
            confirmed_sat: 0,
            created_at: new.created_at,
            expires_at: new.expires_at,
            settled_at: None,
            creator: new.creator.to_string(),
        })
    }

    /// Run arbitrary SQL (tests use this to simulate storage damage).
    #[cfg(test)]
    pub(crate) fn execute_raw(&self, sql: &str) {
        self.lock().execute_batch(sql).unwrap();
    }

    /// Look up one invoice.
    pub fn get(&self, id: &str) -> Result<Option<InvoiceRow>> {
        let conn = self.lock();
        let found = conn
            .query_row(
                &format!("SELECT {COLUMNS} FROM invoices WHERE id = ?1"),
                [id],
                row_to_invoice,
            )
            .optional()?;
        found.map(finish).transpose()
    }

    /// Newest-first page of invoices, optionally filtered by status, strictly
    /// below derivation index `before`.
    pub fn list(&self, status: Option<Status>, before: Option<u32>, limit: u32) -> Result<Vec<InvoiceRow>> {
        let conn = self.lock();
        let mut stmt = conn.prepare(&format!(
            "SELECT {COLUMNS} FROM invoices
             WHERE (?1 IS NULL OR status = ?1) AND (?2 IS NULL OR idx < ?2)
             ORDER BY idx DESC LIMIT ?3"
        ))?;
        let rows = stmt.query_map(params![status.map(Status::as_str), before, limit], row_to_invoice)?;
        let mut out = Vec::new();
        for r in rows {
            out.push(finish(r?)?);
        }
        Ok(out)
    }

    /// Invoices the chain monitor must keep watching: open ones, plus expired
    /// ones that expired at or after `expired_since` (late-payment grace).
    pub fn active(&self, expired_since: u64) -> Result<Vec<InvoiceRow>> {
        let conn = self.lock();
        let mut stmt = conn.prepare(&format!(
            "SELECT {COLUMNS} FROM invoices
             WHERE status IN ('pending', 'detected') OR (status = 'expired' AND expires_at >= ?1)
             ORDER BY idx"
        ))?;
        let rows = stmt.query_map([expired_since], row_to_invoice)?;
        let mut out = Vec::new();
        for r in rows {
            out.push(finish(r?)?);
        }
        Ok(out)
    }

    /// Record the latest chain view of an invoice.
    pub fn update(
        &self,
        id: &str,
        status: Status,
        received_sat: u64,
        confirmed_sat: u64,
        settled_at: Option<u64>,
    ) -> Result<()> {
        self.lock().execute(
            "UPDATE invoices SET status = ?2, received_sat = ?3, confirmed_sat = ?4, settled_at = ?5 WHERE id = ?1",
            params![id, status.as_str(), received_sat, confirmed_sat, settled_at],
        )?;
        Ok(())
    }

    /// Number of invoices still awaiting settlement.
    pub fn count_open(&self) -> Result<u64> {
        Ok(self.lock().query_row(
            "SELECT COUNT(*) FROM invoices WHERE status IN ('pending', 'detected')",
            [],
            |r| r.get(0),
        )?)
    }

    /// Number of invoices created at or after `since`.
    pub fn count_created_since(&self, since: u64) -> Result<u64> {
        Ok(self
            .lock()
            .query_row("SELECT COUNT(*) FROM invoices WHERE created_at >= ?1", [since], |r| {
                r.get(0)
            })?)
    }

    /// Aggregate counts and sums.
    pub fn totals(&self) -> Result<Totals> {
        let conn = self.lock();
        let mut t = Totals {
            next_index: conn.query_row(
                "SELECT CAST(value AS INTEGER) FROM meta WHERE key = ?1",
                [NEXT_INDEX],
                |r| r.get(0),
            )?,
            ..Totals::default()
        };
        let mut stmt = conn.prepare(
            "SELECT status, COUNT(*), COALESCE(SUM(confirmed_sat), 0), COALESCE(SUM(received_sat), 0)
             FROM invoices GROUP BY status",
        )?;
        let rows = stmt.query_map([], totals_row)?;
        for r in rows {
            let (status, count, confirmed_sat, received_sat) = r?;
            match Status::parse(&status)? {
                Status::Pending => t.pending = count,
                Status::Detected => {
                    t.detected = count;
                    t.detected_sat = received_sat;
                }
                Status::Confirmed => {
                    t.confirmed = count;
                    t.confirmed_sat = confirmed_sat;
                }
                Status::Expired => t.expired = count,
            }
        }
        Ok(t)
    }

    /// Read a bookkeeping value.
    pub fn meta_get(&self, key: &str) -> Result<Option<String>> {
        Ok(self
            .lock()
            .query_row("SELECT value FROM meta WHERE key = ?1", [key], |r| r.get(0))
            .optional()?)
    }

    /// Write (or with `None`, delete) a bookkeeping value.
    pub fn meta_set(&self, key: &str, value: Option<&str>) -> Result<()> {
        let conn = self.lock();
        match value {
            Some(v) => conn.execute(UPSERT_META, params![key, v])?,
            None => conn.execute("DELETE FROM meta WHERE key = ?1", [key])?,
        };
        Ok(())
    }

    /// Tie the database to one wallet account key. Invoice addresses are
    /// numbered per database, so a database reused with another key would hand
    /// out that key's addresses far beyond what a wallet scans by default.
    pub fn bind_account(&self, id: &str) -> Result<()> {
        match self.meta_get(ACCOUNT)? {
            None => self.meta_set(ACCOUNT, Some(id)),
            Some(bound) if bound == id => Ok(()),
            Some(_) => Err(Error::Config(
                "this database was used with a different wallet key (DEADHOP_XPUB). Switch back to that key, \
                 or start the new key with a fresh data directory"
                    .into(),
            )),
        }
    }

    /// Time of the earliest invoice, if any.
    pub fn first_created_at(&self) -> Result<Option<u64>> {
        Ok(self
            .lock()
            .query_row("SELECT MIN(created_at) FROM invoices", [], |r| r.get(0))?)
    }

    /// Remember a request event id until `forget_at`. Returns `false` if it
    /// was already used. Expired entries are pruned on the way.
    pub fn remember_request(&self, id: &str, forget_at: u64, now: u64) -> Result<bool> {
        let conn = self.lock();
        conn.execute("DELETE FROM used_requests WHERE forget_at < ?1", [now])?;
        let added = conn.execute(REMEMBER_REQUEST, params![id, forget_at])?;
        Ok(added == 1)
    }

    /// Append to the audit log.
    pub fn audit(&self, at: u64, actor: &str, action: &str, detail: &str) -> Result<()> {
        self.lock().execute(
            "INSERT INTO audit (at, actor, action, detail) VALUES (?1, ?2, ?3, ?4)",
            params![at, actor, action, detail],
        )?;
        Ok(())
    }

    /// Newest audit entries first.
    pub fn audit_recent(&self, limit: u32) -> Result<Vec<AuditRow>> {
        let conn = self.lock();
        let mut stmt = conn.prepare("SELECT at, actor, action, detail FROM audit ORDER BY id DESC LIMIT ?1")?;
        let rows = stmt.query_map([limit], audit_row)?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// Operator keys added from the dashboard, oldest first.
    pub fn operators(&self) -> Result<Vec<OperatorRow>> {
        let conn = self.lock();
        let mut stmt =
            conn.prepare("SELECT pubkey, label, added_at, added_by FROM operators ORDER BY added_at, pubkey")?;
        let rows = stmt.query_map([], operator_row)?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// Add (or relabel) an operator key.
    pub fn add_operator(&self, row: &OperatorRow) -> Result<()> {
        self.lock().execute(
            "INSERT INTO operators (pubkey, label, added_at, added_by) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(pubkey) DO UPDATE SET label = excluded.label",
            params![row.pubkey, row.label, row.added_at, row.added_by],
        )?;
        Ok(())
    }

    /// Remove an operator key. Returns whether it existed.
    pub fn remove_operator(&self, pubkey: &str) -> Result<bool> {
        Ok(self
            .lock()
            .execute("DELETE FROM operators WHERE pubkey = ?1", [pubkey])?
            == 1)
    }

    /// Number and sum of confirmed payments settled at or after `since`.
    pub fn payments_since(&self, since: u64) -> Result<(u64, u64)> {
        Ok(self.lock().query_row(
            "SELECT COUNT(*), COALESCE(SUM(confirmed_sat), 0) FROM invoices
             WHERE status = 'confirmed' AND settled_at >= ?1",
            [since],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?)
    }

    /// Confirmed payments in `n` consecutive buckets of `width` seconds from
    /// `start`, as (count, sum) per bucket; empty buckets are zero.
    pub fn payment_buckets(&self, start: u64, width: u64, n: usize) -> Result<Vec<(u64, u64)>> {
        let end = start + width * n as u64;
        let conn = self.lock();
        let mut stmt = conn.prepare(
            "SELECT (settled_at - ?1) / ?2, COUNT(*), COALESCE(SUM(confirmed_sat), 0) FROM invoices
             WHERE status = 'confirmed' AND settled_at >= ?1 AND settled_at < ?3
             GROUP BY 1",
        )?;
        let rows = stmt.query_map(params![start, width, end], bucket_row)?;
        let mut out = vec![(0, 0); n];
        for r in rows {
            let (bucket, count, sum) = r?;
            out[bucket as usize] = (count, sum);
        }
        Ok(out)
    }

    /// Invoices a customer might still pay: open ones and expired ones still
    /// inside the late-payment grace period.
    pub fn count_watched(&self, expired_since: u64) -> Result<u64> {
        Ok(self.lock().query_row(
            "SELECT COUNT(*) FROM invoices
             WHERE status IN ('pending', 'detected') OR (status = 'expired' AND expires_at >= ?1)",
            [expired_since],
            |r| r.get(0),
        )?)
    }

    /// SQLite's quick integrity check plus size figures.
    pub fn check(&self) -> Result<DbCheck> {
        let conn = self.lock();
        let verdict: String = conn.query_row("PRAGMA quick_check", [], |r| r.get(0))?;
        let pages: u64 = conn.query_row("PRAGMA page_count", [], |r| r.get(0))?;
        let page_size: u64 = conn.query_row("PRAGMA page_size", [], |r| r.get(0))?;
        let invoices: u64 = conn.query_row("SELECT COUNT(*) FROM invoices", [], |r| r.get(0))?;
        Ok(DbCheck {
            ok: verdict == "ok",
            size_bytes: pages * page_size,
            invoices,
        })
    }

    /// Write a consistent copy of the whole database to `path`, which must
    /// not exist yet.
    pub fn backup_to(&self, path: &Path) -> Result<()> {
        let target = path
            .to_str()
            .ok_or_else(|| Error::Config("backup path is not valid UTF-8".into()))?;
        self.lock().execute("VACUUM INTO ?1", [target])?;
        Ok(())
    }

    /// Delete every invoice (and with `settings`, the stored setting
    /// overrides). The address high-water mark, first-start time, operators,
    /// used request ids (replay protection) and the audit log are kept. Returns how many invoices were removed.
    pub fn wipe(&self, settings_key: Option<&str>) -> Result<u64> {
        let mut conn = self.lock();
        let tx = conn.transaction()?;
        let removed = tx.execute("DELETE FROM invoices", [])? as u64;
        if let Some(key) = settings_key {
            tx.execute("DELETE FROM meta WHERE key = ?1", [key])?;
        }
        tx.commit()?;
        Ok(removed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn new(id: &str, created_at: u64, expires_at: u64) -> NewInvoice<'_> {
        NewInvoice {
            id,
            amount_sat: 10_000,
            memo: "m",
            created_at,
            expires_at,
            creator: "c",
        }
    }

    fn addr(i: u32) -> Result<String> {
        Ok(format!("addr{i}"))
    }

    #[test]
    fn insert_get_and_sequential_indices() {
        let db = Db::open_in_memory().unwrap();
        let a = db.insert(&new("a", 1, 100), &addr).unwrap();
        let b = db.insert(&new("b", 2, 100), &addr).unwrap();
        assert_eq!((a.index, b.index), (0, 1));
        assert_eq!(a.address, "addr0");
        assert_eq!(a.status, Status::Pending);
        assert_eq!(db.get("a").unwrap().unwrap(), a);
        assert_eq!(db.get("zzz").unwrap(), None);
        // Duplicate id is refused and does not burn an index.
        assert!(matches!(db.insert(&new("a", 3, 100), &addr), Err(Error::Db(_))));
        // A failing derivation rolls everything back.
        let e = db.insert(&new("c", 3, 100), &|_| Err(Error::Capacity)).unwrap_err();
        assert_eq!(e.code(), "capacity");
        assert_eq!(db.insert(&new("d", 3, 100), &addr).unwrap().index, 2);
    }

    #[test]
    fn update_list_active_and_totals() {
        let db = Db::open_in_memory().unwrap();
        for (i, id) in ["a", "b", "c", "d", "e"].iter().enumerate() {
            db.insert(&new(id, i as u64, 100 + i as u64), &addr).unwrap();
        }
        db.update("b", Status::Detected, 10_000, 0, None).unwrap();
        db.update("c", Status::Confirmed, 12_000, 12_000, Some(55)).unwrap();
        db.update("d", Status::Expired, 0, 0, None).unwrap();
        db.update("e", Status::Expired, 0, 0, None).unwrap();

        let c = db.get("c").unwrap().unwrap();
        assert_eq!(
            (c.status, c.received_sat, c.confirmed_sat, c.settled_at),
            (Status::Confirmed, 12_000, 12_000, Some(55))
        );

        let ids = |rows: Vec<InvoiceRow>| rows.into_iter().map(|r| r.id).collect::<Vec<_>>();
        assert_eq!(ids(db.list(None, None, 10).unwrap()), ["e", "d", "c", "b", "a"]);
        assert_eq!(ids(db.list(None, None, 2).unwrap()), ["e", "d"]);
        assert_eq!(ids(db.list(None, Some(3), 2).unwrap()), ["c", "b"]);
        assert_eq!(ids(db.list(Some(Status::Expired), None, 10).unwrap()), ["e", "d"]);
        assert_eq!(
            ids(db.list(Some(Status::Pending), Some(0), 10).unwrap()),
            Vec::<String>::new()
        );

        // d expired at 103, e at 104.
        assert_eq!(ids(db.active(104).unwrap()), ["a", "b", "e"]);
        assert_eq!(ids(db.active(0).unwrap()), ["a", "b", "d", "e"]);
        assert_eq!(ids(db.active(999).unwrap()), ["a", "b"]);
        assert_eq!(db.count_open().unwrap(), 2);
        assert_eq!(db.count_created_since(0).unwrap(), 5);
        assert_eq!(db.count_created_since(3).unwrap(), 2);
        assert_eq!(db.count_created_since(99).unwrap(), 0);

        assert_eq!(
            db.totals().unwrap(),
            Totals {
                pending: 1,
                detected: 1,
                confirmed: 1,
                expired: 2,
                confirmed_sat: 12_000,
                detected_sat: 10_000,
                next_index: 5
            }
        );
        assert_eq!(Db::open_in_memory().unwrap().totals().unwrap(), Totals::default());
    }

    #[test]
    fn persists_on_disk_and_reports_corruption() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("deadhop.sqlite");
        {
            let db = Db::open(&path).unwrap();
            db.insert(&new("a", 1, 100), &addr).unwrap();
        }
        // A database that was left readable by others is tightened on open.
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
        let db = Db::open(&path).unwrap();
        assert_eq!(std::fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600);
        assert_eq!(db.get("a").unwrap().unwrap().index, 0);

        db.execute_raw("UPDATE invoices SET status = 'weird'");
        assert!(db.get("a").unwrap_err().to_string().contains("corrupt status"));
        assert!(db.list(None, None, 5).is_err());
        assert!(db.totals().is_err());
        db.execute_raw("UPDATE invoices SET status = 'expired'");
        assert_eq!(db.active(0).unwrap().len(), 1);
        db.execute_raw("UPDATE invoices SET status = 'weird'");
        assert!(
            db.active(0).unwrap().is_empty(),
            "rows in an unknown state are not watched"
        );

        assert!(Db::open(dir.path()).is_err(), "a directory is not a database");
    }

    #[test]
    fn every_query_reports_a_damaged_database() {
        let db = Db::open_in_memory().unwrap();
        db.execute_raw("DROP TABLE invoices");
        assert!(db.insert(&new("a", 1, 100), &addr).is_err());
        assert!(db.get("a").is_err());
        assert!(db.list(None, None, 5).is_err());
        assert!(db.active(0).is_err());
        assert!(db.update("a", Status::Expired, 0, 0, None).is_err());
        assert!(db.count_open().is_err());
        assert!(db.count_created_since(0).is_err());
        assert!(db.totals().is_err());

        assert!(db.first_created_at().is_err());
        assert!(db.payments_since(0).is_err());
        assert!(db.payment_buckets(0, 10, 3).is_err());
        assert!(db.count_watched(0).is_err());
        assert!(db.check().is_err());
        assert!(db.wipe(None).is_err());

        // Damage that only the aggregate query notices.
        let db = Db::open_in_memory().unwrap();
        db.execute_raw("ALTER TABLE invoices RENAME COLUMN received_sat TO lost");
        assert!(db.totals().is_err());

        // The bookkeeping tables.
        let db = Db::open_in_memory().unwrap();
        db.execute_raw("DROP TABLE meta; DROP TABLE used_requests; DROP TABLE operators; DROP TABLE audit");
        assert!(db.insert(&new("a", 1, 100), &addr).is_err());
        assert!(db.totals().is_err());
        assert!(db.meta_get("k").is_err());
        assert!(db.meta_set("k", Some("v")).is_err());
        assert!(db.meta_set("k", None).is_err());
        assert!(db.remember_request("e", 1, 0).is_err());
        assert!(db.audit(1, "a", "b", "c").is_err());
        assert!(db.audit_recent(5).is_err());
        assert!(db.operators().is_err());
        assert!(db.remove_operator("x").is_err());
        assert!(db
            .add_operator(&OperatorRow {
                pubkey: "x".into(),
                label: String::new(),
                added_at: 1,
                added_by: "y".into()
            })
            .is_err());
        assert!(db.wipe(Some("settings")).is_err());

        // A column type that does not fit the row structs.
        let db = Db::open_in_memory().unwrap();
        db.execute_raw("INSERT INTO audit (at, actor, action, detail) VALUES (-1, 'a', 'b', 'c')");
        db.execute_raw("INSERT INTO operators VALUES ('k', 'l', -1, 'm')");
        assert!(db.audit_recent(5).is_err());
        assert!(db.operators().is_err());
        db.execute_raw("UPDATE invoices SET settled_at = -1");
        db.insert(&new("a", 1, 100), &addr).unwrap();
        db.execute_raw("UPDATE invoices SET status = 'confirmed', settled_at = 5, confirmed_sat = -3");
        assert!(db.payment_buckets(0, 10, 3).is_err());
    }

    #[test]
    fn indices_never_go_backwards_and_bookkeeping_works() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("deadhop.sqlite");
        let db = Db::open(&path).unwrap();
        assert_eq!(db.totals().unwrap().next_index, 0);
        assert_eq!(db.first_created_at().unwrap(), None);
        for (i, id) in ["a", "b", "c"].iter().enumerate() {
            db.insert(&new(id, 10 + i as u64, 100), &addr).unwrap();
        }
        assert_eq!(db.first_created_at().unwrap(), Some(10));
        db.update("a", Status::Confirmed, 500, 500, Some(1_000)).unwrap();
        db.update("b", Status::Confirmed, 700, 700, Some(1_030)).unwrap();
        db.update("c", Status::Expired, 0, 0, None).unwrap();
        assert_eq!(db.payments_since(1_000).unwrap(), (2, 1_200));
        assert_eq!(db.payments_since(1_001).unwrap(), (1, 700));
        assert_eq!(
            db.payment_buckets(990, 20, 3).unwrap(),
            vec![(1, 500), (0, 0), (1, 700)]
        );
        assert_eq!(db.payment_buckets(2_000, 20, 2).unwrap(), vec![(0, 0), (0, 0)]);
        assert_eq!(db.count_watched(0).unwrap(), 1, "expired within grace is still watched");
        assert_eq!(db.count_watched(1_000).unwrap(), 0);

        let check = db.check().unwrap();
        assert!(check.ok && check.size_bytes > 0);
        assert_eq!(check.invoices, 3);

        // Backups are complete, standalone copies.
        let copy = dir.path().join("copy.sqlite");
        db.backup_to(&copy).unwrap();
        assert_eq!(Db::open(&copy).unwrap().totals().unwrap().confirmed, 2);
        assert!(db.backup_to(&copy).is_err(), "never overwrites");
        use std::os::unix::ffi::OsStrExt;
        let bad = std::path::Path::new(std::ffi::OsStr::from_bytes(b"/tmp/\xff.sqlite"));
        assert!(db.backup_to(bad).unwrap_err().to_string().contains("UTF-8"));

        // Wiping keeps the high-water mark: the next invoice gets a new index.
        db.meta_set("settings", Some("{}")).unwrap();
        assert_eq!(db.wipe(Some("settings")).unwrap(), 3);
        assert_eq!(db.meta_get("settings").unwrap(), None);
        assert_eq!(db.totals().unwrap().next_index, 3);
        assert_eq!(db.insert(&new("d", 20, 100), &addr).unwrap().index, 3);
        drop(db);
        let db = Db::open(&path).unwrap();
        assert_eq!(db.insert(&new("e", 21, 100), &addr).unwrap().index, 4);
        db.meta_set("k", Some("v")).unwrap();
        assert_eq!(db.wipe(None).unwrap(), 2);
        assert_eq!(db.meta_get("k").unwrap().as_deref(), Some("v"));
        db.meta_set("k", None).unwrap();
        assert_eq!(db.meta_get("k").unwrap(), None);

        // Used request ids are remembered until they could no longer be accepted.
        assert!(db.remember_request("e1", 100, 0).unwrap());
        assert!(!db.remember_request("e1", 100, 50).unwrap());
        assert!(db.remember_request("e1", 300, 101).unwrap(), "pruned after forget_at");

        // Operators and the audit log.
        let op = |k: &str, at: u64| OperatorRow {
            pubkey: k.into(),
            label: "till".into(),
            added_at: at,
            added_by: "boss".into(),
        };
        db.add_operator(&op("k2", 2)).unwrap();
        db.add_operator(&op("k1", 1)).unwrap();
        db.add_operator(&OperatorRow {
            label: "renamed".into(),
            ..op("k1", 9)
        })
        .unwrap();
        let ops = db.operators().unwrap();
        assert_eq!(ops.iter().map(|o| o.pubkey.as_str()).collect::<Vec<_>>(), ["k1", "k2"]);
        assert_eq!(
            (ops[0].label.as_str(), ops[0].added_at, ops[0].added_by.as_str()),
            ("renamed", 1, "boss")
        );
        assert!(db.remove_operator("k1").unwrap());
        assert!(!db.remove_operator("k1").unwrap());
        db.audit(5, "boss", "stop", "").unwrap();
        db.audit(6, "boss", "start", "x").unwrap();
        let log = db.audit_recent(1).unwrap();
        assert_eq!(
            log,
            vec![AuditRow {
                at: 6,
                actor: "boss".into(),
                action: "start".into(),
                detail: "x".into()
            }]
        );
    }

    #[test]
    fn a_database_belongs_to_one_wallet() {
        let db = Db::open_in_memory().unwrap();
        db.bind_account("a").unwrap();
        db.bind_account("a").unwrap();
        assert!(db
            .bind_account("b")
            .unwrap_err()
            .to_string()
            .contains("different wallet key"));
    }

    #[test]
    fn an_old_database_gets_a_high_water_mark() {
        let db = Db::open_in_memory().unwrap();
        db.insert(&new("a", 1, 100), &addr).unwrap();
        db.execute_raw("DELETE FROM meta");
        // As if opened for the first time by this version.
        let conn = std::mem::replace(&mut *db.lock(), Connection::open_in_memory().unwrap());
        let db = Db::init(conn).unwrap();
        assert_eq!(db.totals().unwrap().next_index, 1);
    }

    #[test]
    fn survives_a_poisoned_lock() {
        let db = std::sync::Arc::new(Db::open_in_memory().unwrap());
        let d2 = db.clone();
        let _ = std::thread::spawn(move || {
            let _guard = d2.conn.lock().unwrap();
            panic!("poison");
        })
        .join();
        assert_eq!(db.count_open().unwrap(), 0);
    }

    #[test]
    fn status_strings_round_trip() {
        for s in [Status::Pending, Status::Detected, Status::Confirmed, Status::Expired] {
            assert_eq!(Status::parse(s.as_str()).unwrap(), s);
            assert_eq!(serde_json::to_string(&s).unwrap(), format!("\"{}\"", s.as_str()));
        }
    }
}
