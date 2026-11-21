//! Process wiring: keys, storage, chain backend, monitor loop and transport.

use std::fs;
use std::future::Future;
use std::io::{stdout, ErrorKind, Read, Write};
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt};
use std::path::Path;
use std::process::ExitCode;
use std::sync::Arc;
use std::time::Duration;

use bitcoin::blockdata::constants::genesis_block;
use contextvm_sdk::transport::server::{NostrServerTransport, NostrServerTransportConfig};
use contextvm_sdk::{EncryptionMode, GiftWrapMode, ServerInfo};
use nostr_sdk::{Keys, RelayStatus, ToBech32};
use rmcp::service::RoleServer;
use rmcp::transport::{IntoTransport, Transport};
use rmcp::ServiceExt;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

use crate::config::Config;
use crate::db::Db;
use crate::electrum::{Chain, Electrum};
use crate::error::{Error, Result};
use crate::handshake::LocalHandshake;
use crate::keyholder;
use crate::manage::BACKUP_DIR;
use crate::mcp::PayServer;
use crate::proxypool::Connection;
use crate::relay::{parse_proxy, Pool};
use crate::service::{system_clock, RelayProbe, RelayState, Service};
use crate::wallet::Wallet;

/// File (inside the data directory) holding the server's nostr secret key.
pub const KEY_FILE: &str = "server.nsec";
/// File the server writes its public key to, for operators and tooling.
pub const PUBKEY_FILE: &str = "server.pubkey";
/// Database file name.
pub const DB_FILE: &str = "deadhop.sqlite";
/// The project's name before it became deadhop. Data directories from then
/// hold `cvmpay.sqlite` and `backups/cvmpay-*.sqlite`.
const OLD_NAME: &str = "cvmpay";
/// Longest wait for one exchange with the Electrum server.
const ELECTRUM_TIMEOUT: Duration = Duration::from_secs(20);

/// Move a database and backups made under the project's old name to their
/// new names, so an existing deployment keeps its invoices and addresses.
/// Does nothing when there is nothing to move or the new database exists.
pub fn rename_old_files(data_dir: &Path) -> Result<()> {
    if !data_dir.join(DB_FILE).exists() {
        for suffix in ["", "-wal", "-shm", "-journal"] {
            let old = data_dir.join(format!("{OLD_NAME}.sqlite{suffix}"));
            if old.exists() {
                fs::rename(old, data_dir.join(format!("{DB_FILE}{suffix}")))?;
            }
        }
    }
    let backups = data_dir.join(BACKUP_DIR);
    let Ok(entries) = fs::read_dir(&backups) else { return Ok(()) };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if let Some(rest) = name
            .strip_prefix(&format!("{OLD_NAME}-"))
            .filter(|r| r.ends_with(".sqlite"))
        {
            fs::rename(backups.join(&name), backups.join(format!("deadhop-{rest}")))?;
        }
    }
    Ok(())
}

/// Read a file that must be private to its owner; `None` if it does not
/// exist. Permissions are read from the open handle, so the file that is
/// checked is the file that is read.
pub fn read_private(path: &Path) -> Result<Option<String>> {
    match fs::File::open(path) {
        Ok(mut file) => {
            if file.metadata()?.permissions().mode() & 0o077 != 0 {
                return Err(Error::Config(format!(
                    "{} is accessible by other users; run: chmod 600 {}",
                    path.display(),
                    path.display()
                )));
            }
            let mut text = String::new();
            file.read_to_string(&mut text)?;
            Ok(Some(text))
        }
        Err(e) if e.kind() == ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.into()),
    }
}

/// Create an owner-only file holding `content`; refuses to replace one.
/// Synced to disk, so a secret survives a crash right after it is made.
pub fn create_private(path: &Path, content: &str) -> Result<()> {
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)?;
    file.write_all(content.as_bytes())?;
    file.sync_all()?;
    Ok(())
}

/// Load the server identity, creating it on first start. The file must be
/// private to the owner; a key readable by others is refused.
pub fn load_or_create_keys(path: &Path) -> Result<Keys> {
    if let Some(text) = read_private(path)? {
        return Keys::parse(text.trim())
            .map_err(|_| Error::Config(format!("{} does not contain a nostr secret key", path.display())));
    }
    let keys = Keys::generate();
    let nsec = keys
        .secret_key()
        .to_bech32()
        .expect("a valid secret key always encodes as bech32");
    create_private(path, &nsec)?;
    Ok(keys)
}

/// Create the data directory (owner-only) if needed and load the server
/// identity from it, creating that on first start too.
pub fn open_identity(data_dir: &Path) -> Result<Keys> {
    open_dir(data_dir).and_then(|()| load_or_create_keys(&data_dir.join(KEY_FILE)))
}

/// Create a directory if needed and make it private to the owner.
pub fn open_dir(dir: &Path) -> Result<()> {
    fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(dir)
        .and_then(|()| fs::set_permissions(dir, fs::Permissions::from_mode(0o700)))
        .map_err(Error::from)
}

/// Seconds since the Unix epoch.
fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// The data directory: `DEADHOP_DATA_DIR`, or `data` when unset.
fn data_dir(value: Option<String>) -> std::path::PathBuf {
    value.unwrap_or("data".into()).into()
}

/// Print the service's public key (hex), creating its identity if needed.
/// Lets a deployment publish the key (for the dashboard's config) before the
/// service starts, without giving anything else access to the data directory.
pub fn pubkey(data_dir: &Path, out: &mut dyn Write) -> ExitCode {
    match open_identity(data_dir) {
        Ok(keys) => {
            let _ = writeln!(out, "{}", keys.public_key().to_hex());
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("deadhop: {e}");
            ExitCode::FAILURE
        }
    }
}

/// Poll the chain forever, at the live `poll_secs` setting unless `every` is given.
/// How often every live relay is checked end to end.
#[cfg(not(test))]
const RELAY_HEALTH_EVERY: Duration = Duration::from_secs(300);
#[cfg(test)]
const RELAY_HEALTH_EVERY: Duration = Duration::from_millis(500);

/// Check the live relays now and then, for `health` and the dashboard.
async fn relay_health_loop(svc: Arc<Service>) {
    loop {
        svc.refresh_relay_health().await;
        tokio::time::sleep(RELAY_HEALTH_EVERY).await;
    }
}

pub async fn monitor(svc: Arc<Service>, every: Option<Duration>) {
    loop {
        if let Err(e) = svc.poll_once().await {
            tracing::warn!(error = %e, "chain poll failed");
        }
        tokio::time::sleep(every.unwrap_or_else(|| Duration::from_secs(svc.poll_secs()))).await;
    }
}

/// How long a new relay list gets to connect before it is reverted.
#[cfg(not(test))]
const RELAY_CHECK: Duration = Duration::from_secs(30);
#[cfg(test)]
const RELAY_CHECK: Duration = Duration::from_secs(3);
/// How often the supervisor looks at the live relays.
#[cfg(not(test))]
const RELAY_WATCH_EVERY: Duration = Duration::from_secs(30);
#[cfg(test)]
const RELAY_WATCH_EVERY: Duration = Duration::from_millis(250);
/// How long no relay may be connected before the service goes back to the
/// relays and proxy from the environment (issue #6).
#[cfg(not(test))]
const RELAY_DEAD_AFTER: Duration = Duration::from_secs(600);
#[cfg(test)]
const RELAY_DEAD_AFTER: Duration = Duration::from_secs(2);
/// Audit-log reasons for going back to the environment's relays.
const DEAD_RELAYS: &str = "no relay was connected for 10 minutes; back to the relays from the environment";
const DEAD_AT_START: &str = "no saved relay connected at startup; using the relays from the environment";
/// Pause before a restart, so the reply to the request that caused it is
/// published on the old connections first.
#[cfg(not(test))]
const RESTART_GRACE: Duration = Duration::from_millis(1500);
#[cfg(test)]
const RESTART_GRACE: Duration = Duration::from_millis(200);

/// Relay states read from the live relay pool.
struct PoolProbe(Arc<Pool>);

#[async_trait::async_trait]
impl RelayProbe for PoolProbe {
    async fn states(&self) -> Vec<RelayState> {
        let mut out: Vec<RelayState> = self
            .0
            .client()
            .relays()
            .await
            .into_iter()
            .map(|(url, relay)| {
                let status = relay.status();
                RelayState {
                    url: url.to_string(),
                    status: status.to_string().to_lowercase(),
                    connected: status == RelayStatus::Connected,
                }
            })
            .collect();
        out.sort_by(|a, b| a.url.cmp(&b.url));
        out
    }
}

/// One generation of relay connections and the request loop on top of them.
struct Link {
    relays: Vec<String>,
    proxy: String,
    pool: Arc<Pool>,
    cancel: CancellationToken,
    /// Cancelled by the request loop when it ends, for whatever reason.
    ended: CancellationToken,
    task: JoinHandle<()>,
}

impl Link {
    async fn up(keys: &Keys, cfg: &Config, relays: Vec<String>, proxy: String, svc: &Arc<Service>) -> Result<Link> {
        let sink: crate::relay::RelaySink = {
            let svc = svc.clone();
            Arc::new(move |relay, ok, ms| svc.record_relay_event(relay, "send", ok, Some(ms)))
        };
        let pool = Arc::new(Pool::new(keys.clone(), parse_proxy(&proxy)?).with_sink(sink));
        let transport_config = NostrServerTransportConfig::default()
            .with_relay_urls(relays.clone())
            .with_encryption_mode(EncryptionMode::Required)
            .with_gift_wrap_mode(GiftWrapMode::Optional)
            .with_announced_server(cfg.announce)
            .with_publish_relay_list(cfg.announce)
            // Never publish to relays the operator did not choose.
            .with_bootstrap_relay_urls(Vec::new())
            // Requests are a few hundred bytes; do not buffer multi-megabyte
            // reassemblies for anonymous peers.
            .with_oversized_enabled(false)
            .with_server_info(ServerInfo::default().with_name(svc.settings().name.clone()));
        let transport = NostrServerTransport::with_relay_pool(transport_config, pool.clone()).await?;
        let transport = LocalHandshake::new(IntoTransport::<RoleServer, _, _>::into_transport(transport));
        let cancel = CancellationToken::new();
        let ended = CancellationToken::new();
        let task = tokio::spawn({
            let (server, cancel, ended) = (PayServer::new(svc.clone()), cancel.clone(), ended.clone());
            async move {
                let outcome = serve(server, transport, cancel).await;
                tracing::info!(?outcome, "request loop ended");
                ended.cancel();
            }
        });
        svc.set_relay_probe(Some(Arc::new(PoolProbe(pool.clone()))));
        Ok(Link {
            relays,
            proxy,
            pool,
            cancel,
            ended,
            task,
        })
    }

    async fn close(self) {
        self.cancel.cancel();
        let _ = self.task.await;
        self.pool.client().disconnect().await;
    }
}

/// Whether any relay of `link` is connected within `limit`.
async fn connects_within(link: &Link, limit: Duration) -> bool {
    let probe = PoolProbe(link.pool.clone());
    let wait = async {
        while !probe.states().await.iter().any(|r| r.connected) {
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    };
    tokio::time::timeout(limit, wait).await.is_ok()
}

/// The proxy url a connection goes through: empty for a straight connection,
/// `None` when there is to be no connection at all.
fn proxy_of(c: &Connection) -> Option<String> {
    match c {
        Connection::Off => None,
        Connection::Direct => Some(String::new()),
        Connection::Via(url) => Some(url.clone()),
    }
}

/// Connect the way the service is set to now: through the chosen proxy, or
/// straight, or (kill switch) not at all.
async fn connect(keys: &Keys, cfg: &Config, svc: &Arc<Service>, relays: Vec<String>) -> Result<Option<Link>> {
    match proxy_of(&svc.connection()) {
        Some(proxy) => Ok(Some(Link::up(keys, cfg, relays, proxy, svc).await?)),
        None => {
            tracing::warn!("no proxy is reachable and the policy is the kill switch: not connected to any relay");
            svc.set_relay_probe(None);
            Ok(None)
        }
    }
}

/// Bring the request loop back up with the live relays. If a new relay list
/// cannot connect, go back to the previous one so the service stays reachable.
/// (Proxies are not reverted here: the pool picks one, or applies the offline
/// policy.)
async fn relink(keys: &Keys, cfg: &Config, svc: &Arc<Service>, previous: Vec<String>) -> Result<Option<Link>> {
    let wanted = svc.settings().relays.clone();
    if wanted == previous {
        return connect(keys, cfg, svc, wanted).await;
    }
    match connect(keys, cfg, svc, wanted).await {
        Ok(Some(link)) if connects_within(&link, RELAY_CHECK).await => return Ok(Some(link)),
        Ok(Some(link)) => link.close().await,
        // Nothing can connect at all (kill switch): nothing to check the new list against.
        Ok(None) => return Ok(None),
        Err(e) => tracing::warn!(error = %e, "could not start the new relay connections"),
    }
    tracing::warn!("no relay connected with the new settings; going back to the previous ones");
    svc.revert_connection(previous.clone(), "no relay connected with the new relays")?;
    connect(keys, cfg, svc, previous).await
}

/// Drop the saved relay override and connect with the environment's relays.
/// They are under the host's control, so they are the way back when the saved
/// ones cannot be reached (and with them, no remote management either).
async fn fall_back(keys: &Keys, cfg: &Config, svc: &Arc<Service>, why: &str) -> Result<Option<Link>> {
    svc.revert_connection(cfg.relays.clone(), why)?;
    connect(keys, cfg, svc, cfg.relays.clone()).await
}

/// Whether any relay of `link` is connected right now.
async fn any_connected(link: &Link) -> bool {
    PoolProbe(link.pool.clone()).states().await.iter().any(|r| r.connected)
}

/// Keep the request loop running, restarting it when asked or when the pool
/// moves to another proxy (or to none). Also watch the relays: if none has been
/// connected for [`RELAY_DEAD_AFTER`] and they came from a remote change, go
/// back to the environment's, since every remote way to fix them travels over
/// those same dead relays.
async fn supervise(
    svc: Arc<Service>,
    keys: Keys,
    cfg: Config,
    mut link: Option<Link>,
    stop: CancellationToken,
    failed: CancellationToken,
    current: Arc<std::sync::Mutex<CancellationToken>>,
) {
    let mut watch = tokio::time::interval(RELAY_WATCH_EVERY);
    watch.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut last_connected = tokio::time::Instant::now();
    loop {
        *current.lock().unwrap_or_else(std::sync::PoisonError::into_inner) =
            link.as_ref().map_or_else(CancellationToken::new, |l| l.cancel.clone());
        let ended = link.as_ref().map(|l| l.ended.clone());
        let request_loop_ended = async {
            match &ended {
                Some(t) => t.cancelled().await,
                None => std::future::pending().await,
            }
        };
        tokio::select! {
            _ = stop.cancelled() => {
                if let Some(l) = link.take() {
                    l.close().await;
                }
                return;
            }
            _ = request_loop_ended => {
                tracing::warn!("request loop stopped unexpectedly");
                failed.cancel();
                return;
            }
            _ = svc.restart_requested() => {
                tokio::time::sleep(RESTART_GRACE).await;
                let previous = link.as_ref().map_or_else(|| svc.settings().relays.clone(), |l| l.relays.clone());
                if let Some(l) = link.take() {
                    l.close().await;
                }
                link = match relink(&keys, &cfg, &svc, previous).await {
                    Ok(l) => l,
                    Err(e) => {
                        tracing::error!(error = %e, "could not restart the relay connections");
                        failed.cancel();
                        return;
                    }
                };
                svc.restarted();
                last_connected = tokio::time::Instant::now();
                tracing::info!(relays = link.as_ref().map_or(0, |l| l.relays.len()), "relay connections restarted");
            }
            _ = svc.pool_changed() => {
                // The pool chose another proxy, or none. Do nothing if the
                // connections already match.
                let relays = svc.settings().relays.clone();
                let wanted = proxy_of(&svc.connection());
                let now = link.as_ref().map(|l| l.proxy.clone());
                if now == wanted && link.as_ref().is_none_or(|l| l.relays == relays) {
                    continue;
                }
                tokio::time::sleep(RESTART_GRACE).await;
                if let Some(l) = link.take() {
                    l.close().await;
                }
                link = match connect(&keys, &cfg, &svc, relays).await {
                    Ok(l) => l,
                    Err(e) => {
                        tracing::error!(error = %e, "could not switch the relay connections");
                        failed.cancel();
                        return;
                    }
                };
                svc.restarted();
                last_connected = tokio::time::Instant::now();
                tracing::info!(via_proxy = link.as_ref().is_some_and(|l| !l.proxy.is_empty()), connected = link.is_some(), "relay connections moved");
            }
            _ = watch.tick() => {
                let Some(l) = &link else { continue };
                if any_connected(l).await {
                    last_connected = tokio::time::Instant::now();
                    continue;
                }
                if last_connected.elapsed() < RELAY_DEAD_AFTER {
                    continue;
                }
                last_connected = tokio::time::Instant::now();
                if l.relays == cfg.relays {
                    // Nothing better to go back to; the relay pool keeps retrying.
                    tracing::warn!("no relay has been connected for a while; still retrying them");
                    continue;
                }
                tracing::warn!("no relay has been connected for a while; going back to the environment's relays");
                if let Some(l) = link.take() {
                    l.close().await;
                }
                link = match fall_back(&keys, &cfg, &svc, DEAD_RELAYS).await {
                    Ok(l) => l,
                    Err(e) => {
                        tracing::error!(error = %e, "could not go back to the environment's relays");
                        failed.cancel();
                        return;
                    }
                };
                svc.restarted();
            }
        }
    }
}

/// A started service.
pub struct App {
    /// The service logic (shared with the transport).
    pub svc: Arc<Service>,
    /// Hex public key clients address requests to.
    pub pubkey: String,
    stop: CancellationToken,
    /// Cancelled when the request loop fails and cannot be brought back.
    failed: CancellationToken,
    supervisor: JoinHandle<()>,
    monitor: JoinHandle<()>,
    relay_health: JoinHandle<()>,
    proxy_probe: JoinHandle<()>,
    /// Cancels the current request loop (tests use it to simulate a crash).
    #[cfg_attr(not(test), allow(dead_code))]
    current: Arc<std::sync::Mutex<CancellationToken>>,
}

/// Serve MCP requests until cancelled or until the transport ends. Returns
/// why serving ended, or why it could not start.
async fn serve<T>(server: PayServer, transport: T, cancel: CancellationToken) -> std::result::Result<String, String>
where
    T: Transport<RoleServer> + 'static,
{
    let running = server
        .serve_with_ct(transport, cancel)
        .await
        .map_err(|e| e.to_string())?;
    Ok(format!("{:?}", running.waiting().await))
}

impl App {
    /// Bring everything up. Fails fast on any misconfiguration.
    pub async fn start(cfg: Config) -> Result<App> {
        let keys = open_identity(&cfg.data_dir)?;
        let pubkey = keys.public_key().to_hex();
        let wallet = Wallet::new(&cfg.xpub, cfg.network)?;

        // Refuse to run against a backend on another chain: it would report
        // no (or wrong) payments for our addresses. The same check is then
        // repeated on every reconnection.
        let expected = genesis_block(cfg.network).block_hash().to_string();
        let chain = Electrum::new(cfg.electrum.clone(), ELECTRUM_TIMEOUT).with_security(cfg.electrum_security.clone());
        if chain.genesis_hash().await? != expected {
            return Err(Error::Config(
                "the Electrum server is on a different network than DEADHOP_NETWORK".into(),
            ));
        }
        let chain = Arc::new(chain.expect_genesis(expected));

        rename_old_files(&cfg.data_dir)?;
        let db = Db::open(&cfg.data_dir.join(DB_FILE))?;
        db.bind_account(&wallet.account_id())?;
        let svc = Service::new(cfg.clone(), db, wallet, chain, system_clock(), pubkey.clone());
        let svc = Arc::new(svc?);
        let warnings = svc
            .settings()
            .warnings(cfg.network, &cfg.electrum, &cfg.electrum_security);
        warnings.iter().for_each(|w| tracing::warn!("{w}"));

        // Look at the proxies before connecting anything, so that no relay is
        // ever reached the wrong way round.
        svc.refresh_pool().await;
        let relays = svc.settings().relays.clone();
        let saved = relays != cfg.relays;
        let mut link = connect(&keys, &cfg, &svc, relays).await?;
        // Saved relays that cannot connect would leave the service unreachable,
        // and a restart would not help: fall back to the environment's.
        if saved {
            if let Some(l) = &link {
                if !connects_within(l, RELAY_CHECK).await {
                    tracing::warn!("no saved relay connected; using the relays from the environment");
                    if let Some(l) = link.take() {
                        l.close().await;
                    }
                    link = fall_back(&keys, &cfg, &svc, DEAD_AT_START).await?;
                }
            }
        }
        let (relays, via_proxy) = (
            link.as_ref().map_or_else(Vec::new, |l| l.relays.clone()),
            link.as_ref().is_some_and(|l| !l.proxy.is_empty()),
        );
        let stop = CancellationToken::new();
        let failed = CancellationToken::new();
        let current = Arc::new(std::sync::Mutex::new(
            link.as_ref().map_or_else(CancellationToken::new, |l| l.cancel.clone()),
        ));
        let supervisor = tokio::spawn(supervise(
            svc.clone(),
            keys,
            cfg.clone(),
            link,
            stop.clone(),
            failed.clone(),
            current.clone(),
        ));

        fs::write(cfg.data_dir.join(PUBKEY_FILE), &pubkey)?;
        let monitor = tokio::spawn(monitor(svc.clone(), None));
        let relay_health = tokio::spawn(relay_health_loop(svc.clone()));
        let proxy_probe = tokio::spawn(crate::proxypool::probe_loop(svc.clone(), stop.clone()));
        tracing::info!(%pubkey, network = %cfg.network, relays = relays.len(), via_proxy, paused = svc.is_paused(), "deadhop is serving");
        tracing::info!(address = %svc.first_address()?, "first receive address (index 0): check that your wallet lists it");
        Ok(App {
            svc,
            pubkey,
            stop,
            failed,
            supervisor,
            monitor,
            relay_health,
            proxy_probe,
            current,
        })
    }

    /// Wait for `shutdown`. If the request loop fails first, that is an
    /// error: the process should exit so that its supervisor restarts it,
    /// rather than stay up while answering nobody.
    pub async fn until(&self, shutdown: impl Future<Output = ()>) -> Result<()> {
        tokio::select! {
            _ = shutdown => Ok(()),
            _ = self.failed.cancelled() => Err(Error::Transport("the request loop stopped unexpectedly".into())),
        }
    }

    /// Stop serving.
    pub async fn stop(self) {
        self.monitor.abort();
        self.relay_health.abort();
        self.stop.cancel();
        let _ = self.supervisor.await;
        let _ = self.proxy_probe.await;
    }
}

/// Run until `shutdown` resolves or the request loop fails.
pub async fn run(cfg: Config, shutdown: impl Future<Output = ()>) -> Result<()> {
    let app = App::start(cfg).await?;
    let outcome = app.until(shutdown).await;
    app.stop().await;
    outcome
}

/// Default log filter. The transport library logs every undecryptable event
/// at error level and the MCP library logs every refused request at warning
/// level. Either would let anyone fill the log by sending junk or requests
/// that are bound to be refused, so both are silent unless asked for with
/// `DEADHOP_LOG`.
pub const DEFAULT_LOG_FILTER: &str = "warn,deadhop=info,contextvm_sdk=off,rmcp=off";

/// The log filter: `DEADHOP_LOG`, or the default when it is unset or blank
/// (container platforms often pass optional variables through empty).
pub fn log_filter(value: Option<String>) -> String {
    match value {
        Some(v) if !v.trim().is_empty() => v,
        _ => DEFAULT_LOG_FILTER.into(),
    }
}

/// Resolves on SIGINT or SIGTERM.
pub async fn shutdown_signal() {
    let mut term = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        .expect("SIGTERM handler can be installed");
    tokio::select! {
        _ = tokio::signal::ctrl_c() => {}
        _ = term.recv() => {}
    }
}

/// Usage text for unknown commands.
pub const USAGE: &str = "usage:
  deadhop                          run the service (configured by DEADHOP_* variables)
  deadhop fingerprint HOST:PORT    print the SHA-256 fingerprint of an Electrum server's TLS certificate
  deadhop pubkey                   print the service's public key (hex), creating its identity in DEADHOP_DATA_DIR if needed
  deadhop wallet serve             offline wallet service: generate a wallet on first start, publish its xpub to DEADHOP_XPUB_FILE
  deadhop wallet export            print the wallet's recovery phrase and how to restore it";

/// Read and print the certificate fingerprint of an Electrum server, with the
/// settings that pin it. Trust on first use: compare it with the server.
pub async fn fingerprint(addr: &str, out: &mut dyn Write) -> ExitCode {
    let addr = addr
        .strip_prefix("tls://")
        .or_else(|| addr.strip_prefix("ssl://"))
        .unwrap_or(addr);
    match crate::electrum::fetch_fingerprint(addr, ELECTRUM_TIMEOUT).await {
        Ok(fp) => {
            let text = format!(
                "TLS certificate of {addr}, SHA-256:\n  {fp}\n\nTo pin it:\n  DEADHOP_ELECTRUM=tls://{addr}\n  DEADHOP_ELECTRUM_CERT_SHA256={fp}\n\n\
                 Check it against the certificate on the server first, for example:\n  \
                 openssl x509 -in <fulcrum cert.pem> -noout -fingerprint -sha256\n"
            );
            let _ = out.write_all(text.as_bytes());
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("deadhop: cannot read a TLS certificate from {addr}: {e}");
            ExitCode::FAILURE
        }
    }
}

/// What the binary runs: the service, or a helper command.
pub async fn command(
    args: &[String],
    lookup: impl Fn(&str) -> Option<String>,
    shutdown: impl Future<Output = ()>,
) -> ExitCode {
    match args {
        [] => cli(lookup, shutdown).await,
        [cmd, addr] if cmd == "fingerprint" => fingerprint(addr, &mut std::io::stdout()).await,
        [cmd] if cmd == "pubkey" => pubkey(&data_dir(lookup("DEADHOP_DATA_DIR")), &mut stdout()),
        [a, b] if a == "wallet" && b == "serve" => keyholder::serve(&lookup, Box::pin(shutdown), &mut stdout()).await,
        [a, b] if a == "wallet" && b == "export" => keyholder::export_command(&lookup, unix_now(), &mut stdout()),
        _ => {
            eprintln!("{USAGE}");
            ExitCode::from(2)
        }
    }
}

/// Entry point shared by the binary and the tests.
pub async fn cli(lookup: impl Fn(&str) -> Option<String>, shutdown: impl Future<Output = ()>) -> ExitCode {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::new(log_filter(lookup("DEADHOP_LOG"))))
        .with_writer(std::io::stderr)
        .with_ansi(std::io::IsTerminal::is_terminal(&std::io::stderr()))
        .try_init();
    let outcome = match Config::from_lookup(lookup) {
        Ok(cfg) => run(cfg, shutdown).await,
        Err(e) => Err(e),
    };
    match outcome {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("deadhop: {e}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::tests::{base, load};
    use crate::electrum::testutil::FakeElectrum;
    use crate::pow::leading_zero_bits;
    use crate::service::tests::rig;
    use contextvm_sdk::transport::client::{NostrClientTransport, NostrClientTransportConfig};
    use nostr_relay_builder::MockRelay;
    use nostr_sdk::prelude::{BoxedFuture, NostrSigner, PublicKey, SignerBackend, SignerError, Tag, UnsignedEvent};
    use rmcp::model::{CallToolRequestParams, CallToolResult};
    use rmcp::service::{RoleClient, RunningService, ServiceError};
    use serde_json::{json, Value};
    use std::collections::HashMap;

    /// Client signer that mines NIP-13 work into every event it signs, the
    /// same way the dashboard does in the browser.
    #[derive(Debug)]
    struct MiningSigner {
        keys: Keys,
        bits: u8,
    }

    impl NostrSigner for MiningSigner {
        fn backend(&self) -> SignerBackend<'_> {
            SignerBackend::Keys
        }
        fn get_public_key(&self) -> BoxedFuture<'_, std::result::Result<PublicKey, SignerError>> {
            Box::pin(async move { Ok(self.keys.public_key()) })
        }
        fn sign_event(
            &self,
            unsigned: UnsignedEvent,
        ) -> BoxedFuture<'_, std::result::Result<nostr_sdk::Event, SignerError>> {
            Box::pin(async move {
                let mut nonce = 0u64;
                loop {
                    let mut tags: Vec<Tag> = unsigned.tags.iter().cloned().collect();
                    tags.push(Tag::parse(["nonce".to_string(), nonce.to_string(), self.bits.to_string()]).unwrap());
                    let mut candidate = UnsignedEvent::new(
                        unsigned.pubkey,
                        unsigned.created_at,
                        unsigned.kind,
                        tags,
                        unsigned.content.clone(),
                    );
                    if leading_zero_bits(&candidate.id().to_bytes()) >= u32::from(self.bits) {
                        return candidate.sign_with_keys(&self.keys).map_err(SignerError::backend);
                    }
                    nonce += 1;
                }
            })
        }
        fn nip04_encrypt<'a>(
            &'a self,
            pk: &'a PublicKey,
            content: &'a str,
        ) -> BoxedFuture<'a, std::result::Result<String, SignerError>> {
            self.keys.nip04_encrypt(pk, content)
        }
        fn nip04_decrypt<'a>(
            &'a self,
            pk: &'a PublicKey,
            content: &'a str,
        ) -> BoxedFuture<'a, std::result::Result<String, SignerError>> {
            self.keys.nip04_decrypt(pk, content)
        }
        fn nip44_encrypt<'a>(
            &'a self,
            pk: &'a PublicKey,
            content: &'a str,
        ) -> BoxedFuture<'a, std::result::Result<String, SignerError>> {
            self.keys.nip44_encrypt(pk, content)
        }
        fn nip44_decrypt<'a>(
            &'a self,
            pk: &'a PublicKey,
            content: &'a str,
        ) -> BoxedFuture<'a, std::result::Result<String, SignerError>> {
            self.keys.nip44_decrypt(pk, content)
        }
    }

    struct Stack {
        _relay: MockRelay,
        relay_url: String,
        fake: FakeElectrum,
        dir: tempfile::TempDir,
        admin: Keys,
        env: HashMap<&'static str, String>,
    }

    async fn stack() -> Stack {
        let relay = MockRelay::run().await.unwrap();
        let relay_url = relay.url().await.to_string();
        let fake = FakeElectrum::start().await;
        let dir = tempfile::tempdir().unwrap();
        let admin = Keys::generate();
        let mut env = base();
        env.insert("DEADHOP_RELAYS", relay_url.clone());
        env.insert("DEADHOP_ELECTRUM", fake.addr.clone());
        env.insert(
            "DEADHOP_DATA_DIR",
            dir.path().join("data").to_string_lossy().into_owned(),
        );
        env.insert("DEADHOP_ADMINS", admin.public_key().to_hex());
        env.insert("DEADHOP_PUBLIC_INVOICING", "true".into());
        env.insert("DEADHOP_POW_BITS", "8".into());
        env.insert("DEADHOP_POLL_SECS", "1".into());
        Stack {
            _relay: relay,
            relay_url,
            fake,
            dir,
            admin,
            env,
        }
    }

    type Client = RunningService<RoleClient, ()>;
    const SCHEMA_VIOLATION: i32 = 1;

    fn script_of(address: &str) -> bitcoin::ScriptBuf {
        use std::str::FromStr;
        bitcoin::Address::from_str(address)
            .unwrap()
            .assume_checked()
            .script_pubkey()
    }

    async fn connect<S: nostr_sdk::prelude::IntoNostrSigner>(s: &Stack, server: &str, signer: S) -> Client {
        let transport = NostrClientTransport::new(
            signer,
            NostrClientTransportConfig::default()
                .with_relay_urls(vec![s.relay_url.clone()])
                .with_server_pubkey(server)
                .with_encryption_mode(EncryptionMode::Required),
        )
        .await
        .unwrap();
        ().serve(transport).await.unwrap()
    }

    async fn call(c: &Client, tool: &'static str, args: Value) -> std::result::Result<Value, (i32, String)> {
        let params = CallToolRequestParams::new(tool).with_arguments(args.as_object().unwrap().clone());
        match c.call_tool(params).await {
            Ok(CallToolResult {
                structured_content: Some(v),
                is_error: Some(false),
                ..
            }) => Ok(v),
            // The MCP layer reports schema violations as a tool-level error result.
            Ok(other) => Err((SCHEMA_VIOLATION, format!("{:?}", other.content))),
            Err(e) => {
                let ServiceError::McpError(e) = e else { panic!("transport failure") };
                Err((
                    e.code.0,
                    e.data
                        .map(|d| d["code"].as_str().unwrap().to_string())
                        .unwrap_or_default(),
                ))
            }
        }
    }

    #[tokio::test]
    async fn mining_signer_behaves_like_a_normal_signer() {
        let keys = Keys::generate();
        let peer = Keys::generate();
        let signer = MiningSigner {
            keys: keys.clone(),
            bits: 6,
        };
        assert!(matches!(signer.backend(), SignerBackend::Keys));
        assert_eq!(signer.get_public_key().await.unwrap(), keys.public_key());
        let unsigned = UnsignedEvent::new(
            keys.public_key(),
            nostr_sdk::Timestamp::now(),
            nostr_sdk::Kind::Custom(25910),
            [],
            "{}",
        );
        let event = signer.sign_event(unsigned).await.unwrap();
        event.verify().unwrap();
        assert!(leading_zero_bits(&event.id.to_bytes()) >= 6);
        let c44 = signer.nip44_encrypt(&peer.public_key(), "hi").await.unwrap();
        assert_eq!(peer.nip44_decrypt(&keys.public_key(), &c44).await.unwrap(), "hi");
        let c44 = peer.nip44_encrypt(&keys.public_key(), "yo").await.unwrap();
        assert_eq!(signer.nip44_decrypt(&peer.public_key(), &c44).await.unwrap(), "yo");
        // Legacy NIP-04 encryption is not compiled in; only NIP-44 is used.
        assert!(signer.nip04_encrypt(&peer.public_key(), "old").await.is_err());
        assert!(signer.nip04_decrypt(&peer.public_key(), "old").await.is_err());
    }

    #[test]
    fn files_named_after_the_old_project_are_renamed() {
        let dir = tempfile::tempdir().unwrap();
        let d = dir.path();
        // Nothing to move, and no backups directory: fine.
        rename_old_files(d).unwrap();
        for f in ["cvmpay.sqlite", "cvmpay.sqlite-wal"] {
            fs::write(d.join(f), f).unwrap();
        }
        fs::create_dir(d.join(BACKUP_DIR)).unwrap();
        for f in ["cvmpay-000000000001-000.sqlite", "cvmpay-notes.txt", "other.sqlite"] {
            fs::write(d.join(BACKUP_DIR).join(f), f).unwrap();
        }
        rename_old_files(d).unwrap();
        assert_eq!(fs::read_to_string(d.join(DB_FILE)).unwrap(), "cvmpay.sqlite");
        assert_eq!(
            fs::read_to_string(d.join("deadhop.sqlite-wal")).unwrap(),
            "cvmpay.sqlite-wal"
        );
        assert!(!d.join("cvmpay.sqlite").exists() && !d.join("deadhop.sqlite-shm").exists());
        let mut left: Vec<_> = fs::read_dir(d.join(BACKUP_DIR))
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .collect();
        left.sort();
        assert_eq!(
            left,
            ["cvmpay-notes.txt", "deadhop-000000000001-000.sqlite", "other.sqlite"]
        );

        // A database under the new name is never replaced by an old one.
        fs::write(d.join("cvmpay.sqlite"), "stale").unwrap();
        rename_old_files(d).unwrap();
        assert_eq!(fs::read_to_string(d.join(DB_FILE)).unwrap(), "cvmpay.sqlite");
        assert!(d.join("cvmpay.sqlite").exists());

        // A rename that fails is an error, not a silent fresh start.
        let locked = tempfile::tempdir().unwrap();
        fs::write(locked.path().join("cvmpay.sqlite"), "").unwrap();
        fs::set_permissions(locked.path(), fs::Permissions::from_mode(0o500)).unwrap();
        assert!(rename_old_files(locked.path()).is_err());
        fs::set_permissions(locked.path(), fs::Permissions::from_mode(0o700)).unwrap();
        let ro = tempfile::tempdir().unwrap();
        fs::create_dir(ro.path().join(BACKUP_DIR)).unwrap();
        fs::write(ro.path().join(BACKUP_DIR).join("cvmpay-1.sqlite"), "").unwrap();
        fs::set_permissions(ro.path().join(BACKUP_DIR), fs::Permissions::from_mode(0o500)).unwrap();
        assert!(rename_old_files(ro.path()).is_err());
        fs::set_permissions(ro.path().join(BACKUP_DIR), fs::Permissions::from_mode(0o700)).unwrap();
    }

    #[test]
    fn keys_are_created_private_and_reloaded() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(KEY_FILE);
        let created = load_or_create_keys(&path).unwrap();
        assert_eq!(fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600);
        assert!(fs::read_to_string(&path).unwrap().starts_with("nsec1"));
        assert_eq!(load_or_create_keys(&path).unwrap().public_key(), created.public_key());

        fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
        let e = load_or_create_keys(&path).unwrap_err().to_string();
        assert!(e.contains("accessible by other users"), "{e}");

        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
        fs::write(&path, "not a key").unwrap();
        let e = load_or_create_keys(&path).unwrap_err().to_string();
        assert!(e.contains("does not contain a nostr secret key"), "{e}");

        assert!(load_or_create_keys(&dir.path().join("missing/dir/key")).is_err());
        // A path that runs through a regular file cannot even be looked at.
        assert!(load_or_create_keys(&path.join("key")).is_err());
    }

    #[tokio::test]
    async fn monitor_keeps_polling_through_failures() {
        let r = rig(|_| {}).await;
        r.fake.set(|s| s.error_on.insert("blockchain.headers.subscribe".into()));
        let inv = r.create(5_000);
        let task = tokio::spawn(monitor(r.svc.clone(), Some(Duration::from_millis(10))));
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert!(!r.svc.stats(&r.admin()).unwrap().chain_ok);
        r.fake.set(|s| s.error_on.clear());
        let script = script_of(&inv.address);
        r.fake.pay(&script, 5_000, 0);
        tokio::time::sleep(Duration::from_millis(200)).await;
        task.abort();
        assert!(r.svc.stats(&r.admin()).unwrap().chain_ok);
        assert_eq!(r.svc.stats(&r.admin()).unwrap().detected, 1);
    }

    /// A relay that accepts TCP connections and then never answers: what a
    /// relay hanging behind a proxy looks like.
    async fn black_hole_relay() -> (String, JoinHandle<()>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("ws://{}", listener.local_addr().unwrap());
        let task = tokio::spawn(async move {
            let mut held = Vec::new();
            loop {
                held.push(listener.accept().await.unwrap().0);
            }
        });
        (url, task)
    }

    async fn timed_info(c: &Client) -> Duration {
        let started = std::time::Instant::now();
        call(c, "get_info", json!({})).await.unwrap();
        started.elapsed()
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn one_hanging_relay_never_delays_replies_on_a_working_one() {
        let mut s = stack().await;
        let (hole, _hole_task) = black_hole_relay().await;
        s.env.insert("DEADHOP_RELAYS", format!("{},{hole}", s.relay_url));
        let app = App::start(load(&s.env).unwrap()).await.unwrap();
        let admin = connect(&s, &app.pubkey, s.admin.clone()).await;
        for round in 0..3 {
            let took = timed_info(&admin).await;
            assert!(took < Duration::from_secs(2), "reply {round} took {took:?}");
        }
        // Right after a restart, too.
        call(&admin, "control_service", json!({ "action": "restart" }))
            .await
            .unwrap();
        tokio::time::sleep(Duration::from_secs(3)).await;
        for round in 0..3 {
            let took = timed_info(&admin).await;
            assert!(
                took < Duration::from_secs(2),
                "reply {round} after the restart took {took:?}"
            );
        }
        app.stop().await;
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn full_stack_over_nostr() {
        let s = stack().await;
        let app = App::start(load(&s.env).unwrap()).await.unwrap();
        let data = s.dir.path().join("data");
        assert_eq!(fs::read_to_string(data.join(PUBKEY_FILE)).unwrap(), app.pubkey);
        assert_eq!(fs::metadata(&data).unwrap().permissions().mode() & 0o777, 0o700);

        // --- operator ---
        let admin = connect(&s, &app.pubkey, s.admin.clone()).await;
        let tools = admin.list_all_tools().await.unwrap();
        let mut names: Vec<&str> = tools.iter().map(|t| t.name.as_ref()).collect();
        names.sort();
        assert_eq!(
            names,
            [
                "add_operator",
                "backup_database",
                "check_proxy",
                "check_relays",
                "control_service",
                "create_invoice",
                "get_audit_log",
                "get_health",
                "get_info",
                "get_invoice",
                "get_metrics",
                "get_payment_series",
                "get_privacy_audit",
                "get_relay_history",
                "get_settings",
                "get_stats",
                "list_backups",
                "list_invoices",
                "remove_operator",
                "reset_settings",
                "update_settings",
                "wipe_database"
            ]
        );
        for t in &tools {
            assert!(t.output_schema.is_some(), "{} publishes an output schema", t.name);
        }
        let create = tools.iter().find(|t| t.name == "create_invoice").unwrap();
        assert_eq!(create.input_schema["additionalProperties"], json!(false));

        let info = call(&admin, "get_info", json!({})).await.unwrap();
        assert_eq!(
            (
                info["is_admin"].clone(),
                info["network"].clone(),
                info["pow_bits"].clone()
            ),
            (json!(true), json!("regtest"), json!(8))
        );

        let inv = call(
            &admin,
            "create_invoice",
            json!({"amount_sat": 25_000, "memo": "table 4"}),
        )
        .await
        .unwrap();
        assert_eq!(inv["status"], "pending");
        let id = inv["id"].as_str().unwrap().to_string();
        let address = inv["address"].as_str().unwrap().to_string();

        // The customer pays; the monitor notices on its own.
        s.fake.pay(&script_of(&address), 25_000, 0);
        let mut status = String::new();
        for _ in 0..50 {
            status = call(&admin, "get_invoice", json!({"id": id})).await.unwrap()["status"]
                .as_str()
                .unwrap()
                .to_string();
            if status == "detected" {
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        assert_eq!(status, "detected");

        let list = call(&admin, "list_invoices", json!({"status": "detected"}))
            .await
            .unwrap();
        assert_eq!(list["invoices"][0]["id"], id);
        assert_eq!(list["invoices"][0]["creator"], s.admin.public_key().to_hex());
        let stats = call(&admin, "get_stats", json!({})).await.unwrap();
        assert_eq!(
            (stats["detected"].clone(), stats["chain_ok"].clone()),
            (json!(1), json!(true))
        );

        // Strict typing: unknown fields, wrong types and bad values are refused.
        let (code, why) = call(&admin, "create_invoice", json!({"amount_sat": 25_000, "admin": true}))
            .await
            .unwrap_err();
        assert!(code == SCHEMA_VIOLATION && why.contains("unknown field"), "{why}");
        let (code, why) = call(&admin, "create_invoice", json!({"amount_sat": "lots"}))
            .await
            .unwrap_err();
        assert!(code == SCHEMA_VIOLATION && why.contains("invalid type"), "{why}");
        let (code, why) = call(&admin, "list_invoices", json!({"status": "stolen"}))
            .await
            .unwrap_err();
        assert!(code == SCHEMA_VIOLATION && why.contains("unknown variant"), "{why}");
        assert_eq!(
            call(&admin, "create_invoice", json!({"amount_sat": 1}))
                .await
                .unwrap_err(),
            (-32602, "invalid_params".into())
        );
        assert_eq!(
            call(&admin, "get_invoice", json!({"id": "x"})).await.unwrap_err(),
            (-32602, "invalid_params".into())
        );
        assert_eq!(
            call(&admin, "get_invoice", json!({"id": "0".repeat(32)}))
                .await
                .unwrap_err(),
            (-32002, "not_found".into())
        );

        // --- member of the public who mines proof of work ---
        let miner = connect(
            &s,
            &app.pubkey,
            MiningSigner {
                keys: Keys::generate(),
                bits: 8,
            },
        )
        .await;
        assert_eq!(call(&miner, "get_info", json!({})).await.unwrap()["is_admin"], false);
        let theirs = call(&miner, "create_invoice", json!({"amount_sat": 5_000}))
            .await
            .unwrap();
        assert_eq!(theirs["status"], "pending");
        assert_ne!(theirs["address"], address);
        // They can read an invoice by id but cannot use operator tools.
        assert_eq!(
            call(&miner, "get_invoice", json!({"id": id})).await.unwrap()["memo"],
            "table 4"
        );
        assert_eq!(
            call(&miner, "list_invoices", json!({})).await.unwrap_err(),
            (-32001, "unauthorized".into())
        );
        assert_eq!(
            call(&miner, "get_stats", json!({})).await.unwrap_err(),
            (-32001, "unauthorized".into())
        );

        // --- member of the public without work ---
        let lazy = connect(&s, &app.pubkey, Keys::generate()).await;
        assert_eq!(
            call(&lazy, "create_invoice", json!({"amount_sat": 5_000}))
                .await
                .unwrap_err(),
            (-32003, "pow_rejected".into())
        );

        for c in [admin, miner, lazy] {
            let _ = c.cancel().await;
        }
        app.stop().await;

        // Restart: same identity, same data.
        let app = App::start(load(&s.env).unwrap()).await.unwrap();
        assert_eq!(fs::read_to_string(data.join(PUBKEY_FILE)).unwrap(), app.pubkey);
        let admin_caller = crate::service::Caller {
            pubkey: Some(s.admin.public_key().to_hex()),
            event: None,
        };
        assert_eq!(app.svc.stats(&admin_caller).unwrap().next_index, 2);
        app.stop().await;
    }

    /// An anonymous peer whose first message is not a request must not be
    /// able to break the service for everyone else.
    /// Wait until `f` holds for the operator's view of health.
    async fn health_until(c: &Client, f: impl Fn(&Value) -> bool) -> Value {
        let wait = async {
            loop {
                // Requests sent while the service is between relay links are never answered.
                let reply = tokio::time::timeout(Duration::from_secs(2), call(c, "get_health", json!({}))).await;
                match reply {
                    Ok(Ok(h)) if f(&h) => return h,
                    _ => tokio::time::sleep(Duration::from_millis(100)).await,
                }
            }
        };
        tokio::time::timeout(Duration::from_secs(30), wait)
            .await
            .expect("health reached the expected state")
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn management_over_nostr() {
        let s = stack().await;
        let second = MockRelay::run().await.unwrap();
        let second_url = second.url().await.to_string();
        let app = App::start(load(&s.env).unwrap()).await.unwrap();
        let admin = connect(&s, &app.pubkey, s.admin.clone()).await;

        // Live relay state comes from the relay pool.
        let h = health_until(&admin, |h| h["relays"][0]["connected"] == true).await;
        assert_eq!(h["relays"][0]["status"], "connected");
        assert_eq!(h["database"]["ok"], true);

        // Stop and start invoicing; the public is told why it is refused.
        let public = connect(
            &s,
            &app.pubkey,
            MiningSigner {
                keys: Keys::generate(),
                bits: 8,
            },
        )
        .await;
        call(&admin, "control_service", json!({ "action": "stop" }))
            .await
            .unwrap();
        let refused = call(&public, "create_invoice", json!({ "amount_sat": 5000 }))
            .await
            .unwrap_err();
        assert_eq!(refused, (-32006, "paused".to_string()));
        call(&admin, "control_service", json!({ "action": "start" }))
            .await
            .unwrap();
        call(&public, "create_invoice", json!({ "amount_sat": 5000 }))
            .await
            .unwrap();

        // Add a relay: the service reconnects and keeps answering on the old one.
        let v = call(
            &admin,
            "update_settings",
            json!({ "patch": { "relays": [s.relay_url, second_url] } }),
        )
        .await
        .unwrap();
        assert_eq!(v["restart_pending"], true);
        let h = health_until(&admin, |h| {
            h["service"]["restarts"] == 1 && h["relays"].as_array().unwrap().iter().all(|r| r["connected"] == true)
        })
        .await;
        assert_eq!(h["relays"].as_array().unwrap().len(), 2);

        // A list where nothing connects is reverted, so the service stays reachable.
        call(
            &admin,
            "update_settings",
            json!({ "patch": { "relays": ["ws://127.0.0.1:9"] } }),
        )
        .await
        .unwrap();
        let h = health_until(&admin, |h| h["service"]["restarts"] == 2).await;
        assert_eq!(h["relays"].as_array().unwrap().len(), 2, "{h}");
        let log = call(&admin, "get_audit_log", json!({ "limit": 1 })).await.unwrap();
        assert_eq!(log["entries"][0]["action"], "revert_connection");

        // An explicit restart, and a backup over the wire.
        call(&admin, "control_service", json!({ "action": "restart" }))
            .await
            .unwrap();
        health_until(&admin, |h| h["service"]["restarts"] == 3).await;
        // The proxy check and the privacy audit, over the wire.
        let c = call(&admin, "check_proxy", json!({})).await.unwrap();
        assert_eq!(c["status"], "direct");
        let checked = call(&admin, "check_relays", json!({})).await.unwrap();
        assert!(
            checked["relays"]
                .as_array()
                .unwrap()
                .iter()
                .all(|h| h["status"] == "ok"),
            "{checked}"
        );
        // Replies sent through the relay are kept for the history.
        let hist = call(&admin, "get_relay_history", json!({ "window": "15m" }))
            .await
            .unwrap();
        assert_eq!(
            hist["relays"][0]["relay"].as_str().unwrap().trim_end_matches('/'),
            s.relay_url.trim_end_matches('/')
        );
        assert!(hist["relays"][0]["sends"].as_u64().unwrap() >= 1, "{hist}");
        assert_eq!(hist["relays"][0]["send_failures"], 0, "{hist}");
        let a = call(&admin, "get_privacy_audit", json!({})).await.unwrap();
        assert!(a["checks"]
            .as_array()
            .unwrap()
            .iter()
            .any(|c| c["id"] == "relays_via_proxy"));
        let b = call(&admin, "backup_database", json!({})).await.unwrap();
        assert!(s
            .dir
            .path()
            .join("data/backups")
            .join(b["file"].as_str().unwrap())
            .exists());
        let list = call(&admin, "list_backups", json!({})).await.unwrap();
        assert_eq!(list["backups"][0], b);

        // An operator added by npub can manage too.
        let other = Keys::generate();
        let npub = other.public_key().to_bech32().unwrap();
        call(&admin, "add_operator", json!({ "pubkey": npub, "label": "till" }))
            .await
            .unwrap();
        let v = call(&admin, "reset_settings", json!({})).await.unwrap();
        assert_eq!(v["overridden"].as_array().unwrap().len(), 0);
        let as_other = connect(&s, &app.pubkey, other).await;
        assert!(call(&as_other, "get_metrics", json!({})).await.is_ok());
        assert!(call(&as_other, "get_payment_series", json!({ "range": "week" }))
            .await
            .is_ok());
        assert!(call(&as_other, "get_settings", json!({})).await.is_ok());
        assert_eq!(
            call(&public, "get_settings", json!({})).await.unwrap_err(),
            (-32001, "unauthorized".to_string())
        );
        let v = call(&admin, "remove_operator", json!({ "pubkey": npub }))
            .await
            .unwrap();
        assert_eq!(v["operators"].as_array().unwrap().len(), 1);
        // A wipe without confirmation is refused before anything happens.
        assert_eq!(
            call(&admin, "wipe_database", json!({ "confirm": "no" }))
                .await
                .unwrap_err()
                .1,
            "invalid_params"
        );
        app.stop().await;
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn relay_traffic_goes_through_the_proxy() {
        let mut s = stack().await;
        let (port, _proxy) = crate::relay::tests::socks5_proxy(Some(("svc", "pw"))).await;
        s.env
            .insert("DEADHOP_PROXY", format!("socks5://svc:pw@127.0.0.1:{port}"));
        let app = App::start(load(&s.env).unwrap()).await.unwrap();
        // The operator connects directly; the service only through the proxy.
        let admin = connect(&s, &app.pubkey, s.admin.clone()).await;
        let h = health_until(&admin, |h| h["relays"][0]["connected"] == true).await;
        assert_eq!(h["proxy"], format!("socks5://svc:***@127.0.0.1:{port}"));
        assert_eq!(h["proxy_pool"]["state"], "active");
        let v = call(&admin, "get_settings", json!({})).await.unwrap();
        let shown = v["settings"]["proxies"]["list"][0]["url"].as_str().unwrap();
        assert!(!shown.contains("pw@"), "{shown}");

        // A proxy that is not there is undone, and the one that worked stays.
        call(
            &admin,
            "update_settings",
            json!({ "patch": { "proxies": { "list": [{ "url": "socks5://127.0.0.1:9" }] } } }),
        )
        .await
        .unwrap();
        let h = health_until(&admin, |h| {
            h["proxy_pool"]["proxies"][0]["url"] == format!("socks5://svc:***@127.0.0.1:{port}")
                && h["proxy_pool"]["state"] == "active"
        })
        .await;
        assert_eq!(h["proxy"], format!("socks5://svc:***@127.0.0.1:{port}"));
        let log = call(&admin, "get_audit_log", json!({})).await.unwrap();
        assert!(
            log["entries"]
                .as_array()
                .unwrap()
                .iter()
                .any(|e| e["action"] == "revert_proxies"),
            "{log}"
        );
        // No proxy at all is a straight connection, by the operator's choice.
        call(&admin, "update_settings", json!({ "patch": { "proxy": "" } }))
            .await
            .unwrap();
        let h = health_until(&admin, |h| h["proxy"] == "" && h["relays"][0]["connected"] == true).await;
        assert_eq!(h["proxy_pool"]["state"], "direct");
        app.stop().await;
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn failover_moves_to_the_next_proxy_and_back_again() {
        let mut s = stack().await;
        let (p1, first) = crate::relay::tests::socks5_proxy(None).await;
        let (p2, _second) = crate::relay::tests::socks5_proxy(None).await;
        s.env.insert("DEADHOP_PROXY", format!("socks5://127.0.0.1:{p1}"));
        s.env.insert("DEADHOP_PROXIES", format!("socks5://127.0.0.1:{p2}"));
        let app = App::start(load(&s.env).unwrap()).await.unwrap();
        let admin = connect(&s, &app.pubkey, s.admin.clone()).await;
        let h = health_until(&admin, |h| h["relays"][0]["connected"] == true).await;
        assert_eq!(h["proxy"], format!("socks5://127.0.0.1:{p1}"), "the first in the list");
        // The first dies: the second takes over.
        first.abort();
        let h = health_until(&admin, |h| {
            h["proxy"] == format!("socks5://127.0.0.1:{p2}") && h["relays"][0]["connected"] == true
        })
        .await;
        assert_eq!(h["proxy_pool"]["reachable"], 1);
        assert_eq!(h["proxy_pool"]["proxies"][0]["reachable"], false);
        assert_eq!(h["proxy_pool"]["active"], h["proxy_pool"]["proxies"][1]["id"]);
        app.stop().await;
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn the_kill_switch_disconnects_every_relay_and_reconnects_when_a_proxy_returns() {
        let mut s = stack().await;
        let (port, proxy) = crate::relay::tests::socks5_proxy(None).await;
        s.env.insert("DEADHOP_PROXY", format!("socks5://127.0.0.1:{port}"));
        let app = App::start(load(&s.env).unwrap()).await.unwrap();
        let admin = connect(&s, &app.pubkey, s.admin.clone()).await;
        health_until(&admin, |h| h["relays"][0]["connected"] == true).await;
        proxy.abort();
        let off = {
            let svc = app.svc.clone();
            async move {
                loop {
                    if svc.pool_health().state == crate::proxypool::PoolState::Offline
                        && !svc.relay_states().await.iter().any(|r| r.connected)
                    {
                        return;
                    }
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
            }
        };
        tokio::time::timeout(Duration::from_secs(30), off)
            .await
            .expect("offline with no relay connection");
        assert_eq!(app.svc.connection(), crate::proxypool::Connection::Off);
        assert_eq!(app.svc.posture(), crate::proxypool::Posture::Off);
        // It is not a crash: the service is still there, and comes back with the proxy.
        let (_, _proxy) = crate::relay::tests::socks5_proxy_on(&format!("127.0.0.1:{port}"), None).await;
        let h = health_until(&admin, |h| {
            h["proxy_pool"]["state"] == "active" && h["relays"][0]["connected"] == true
        })
        .await;
        assert_eq!(h["proxy"], format!("socks5://127.0.0.1:{port}"));
        app.stop().await;
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn with_no_proxy_up_the_policy_decides_who_is_served() {
        let mut s = stack().await;
        s.env.insert("DEADHOP_PROXY", "socks5://127.0.0.1:9".into());
        s.env.insert("DEADHOP_PROXY_OFFLINE", "admin_only".into());
        let app = App::start(load(&s.env).unwrap()).await.unwrap();
        // The proxy never worked: connected straight, but for operators only.
        assert_eq!(app.svc.posture(), crate::proxypool::Posture::AdminOnly);
        let admin = connect(&s, &app.pubkey, s.admin.clone()).await;
        let health = health_until(&admin, |h| h["relays"][0]["connected"] == true).await;
        assert_eq!(health["proxy"], "");
        assert_eq!(health["proxy_pool"]["posture"], "admin_only");
        let public = connect(&s, &app.pubkey, Keys::generate()).await;
        assert_eq!(
            call(&public, "get_info", json!({})).await.unwrap_err(),
            (-32007, "unavailable".to_string())
        );
        assert_eq!(call(&admin, "get_info", json!({})).await.unwrap()["is_admin"], true);
        app.stop().await;

        let mut s = stack().await;
        s.env.insert("DEADHOP_PROXY", "socks5://127.0.0.1:9".into());
        s.env.insert("DEADHOP_PROXY_OFFLINE", "invoicing_only".into());
        let app = App::start(load(&s.env).unwrap()).await.unwrap();
        let admin = connect(&s, &app.pubkey, s.admin.clone()).await;
        // Operators are not served; the public can still invoice.
        assert_eq!(
            call(&admin, "get_health", json!({})).await.unwrap_err(),
            (-32001, "unauthorized".to_string())
        );
        let public = connect(&s, &app.pubkey, Keys::generate()).await;
        assert_eq!(call(&public, "get_info", json!({})).await.unwrap()["is_admin"], false);
        app.stop().await;

        let mut s = stack().await;
        s.env.insert("DEADHOP_PROXY", "socks5://127.0.0.1:9".into());
        s.env.insert("DEADHOP_PROXY_OFFLINE", "open".into());
        let app = App::start(load(&s.env).unwrap()).await.unwrap();
        let admin = connect(&s, &app.pubkey, s.admin.clone()).await;
        health_until(&admin, |h| h["relays"][0]["connected"] == true).await;
        let public = connect(&s, &app.pubkey, Keys::generate()).await;
        assert!(call(&public, "get_info", json!({})).await.is_ok());
        app.stop().await;
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_transparent_proxy_is_trusted_without_a_single_check() {
        let mut s = stack().await;
        // A dead proxy is listed, and would take the service offline, if it were used.
        s.env.insert("DEADHOP_PROXY", "socks5://127.0.0.1:9".into());
        s.env.insert("DEADHOP_TRANSPARENT_PROXY", "true".into());
        let app = App::start(load(&s.env).unwrap()).await.unwrap();
        let admin = connect(&s, &app.pubkey, s.admin.clone()).await;
        let h = health_until(&admin, |h| h["relays"][0]["connected"] == true).await;
        assert_eq!(h["proxy"], "");
        assert_eq!(h["proxy_pool"]["state"], "transparent");
        assert_eq!(h["proxy_pool"]["proxies"][0]["reachable"], Value::Null, "never probed");
        let a = call(&admin, "get_privacy_audit", json!({})).await.unwrap();
        let via = a["checks"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["id"] == "relays_via_proxy")
            .unwrap();
        assert_eq!(via["result"], "not_checked");
        assert!(a["checks"].as_array().unwrap().iter().all(|c| c["id"] != "proxy_check"));
        // Asking for a check does not turn into a connection to the proxy either.
        assert_eq!(app.svc.pool_health().proxies[0].checked_at, None);
        app.stop().await;
    }

    #[tokio::test]
    async fn the_fingerprint_command_prints_what_to_pin() {
        let (fake, fp) = FakeElectrum::start_tls().await;
        let mut out = Vec::new();
        let code = fingerprint(&format!("tls://{}", fake.addr), &mut out).await;
        assert_eq!(format!("{code:?}"), format!("{:?}", ExitCode::SUCCESS));
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains(&format!("DEADHOP_ELECTRUM_CERT_SHA256={fp}")), "{text}");
        assert!(text.contains(&format!("DEADHOP_ELECTRUM=tls://{}", fake.addr)));
        let mut out = Vec::new();
        assert!(fingerprint(&format!("ssl://{}", fake.addr), &mut out).await == ExitCode::SUCCESS);
        let plain = FakeElectrum::start().await;
        let mut out = Vec::new();
        assert!(fingerprint(&plain.addr, &mut out).await == ExitCode::FAILURE);
        assert!(out.is_empty());
    }

    #[tokio::test]
    async fn commands_are_dispatched() {
        let none = |_: &str| None;
        assert!(
            command(&[], none, std::future::ready(())).await == ExitCode::FAILURE,
            "unconfigured service"
        );
        let unknown = ["help".to_string()];
        assert!(command(&unknown, none, std::future::ready(())).await == ExitCode::from(2));
        let unknown_wallet = ["wallet".to_string(), "help".to_string()];
        assert!(command(&unknown_wallet, none, std::future::ready(())).await == ExitCode::from(2));
        let dead = std::net::TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .to_string();
        let fp = ["fingerprint".to_string(), dead];
        assert!(command(&fp, none, std::future::ready(())).await == ExitCode::FAILURE);
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("data").to_string_lossy().into_owned();
        let lookup = |k: &str| (k == "DEADHOP_DATA_DIR").then(|| dir.clone());
        let pk = ["pubkey".to_string()];
        assert!(command(&pk, &lookup, std::future::ready(())).await == ExitCode::SUCCESS);
        let wallet = |sub: &str| ["wallet".to_string(), sub.to_string()];
        assert!(command(&wallet("serve"), &lookup, std::future::ready(())).await == ExitCode::SUCCESS);
        assert!(command(&wallet("export"), &lookup, std::future::ready(())).await == ExitCode::SUCCESS);
        assert!(unix_now() > 1_700_000_000);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_database_is_not_reused_with_another_wallet() {
        let mut s = stack().await;
        let app = App::start(load(&s.env).unwrap()).await.unwrap();
        app.stop().await;
        // The BIP-84 test vector's account key, re-encoded for regtest, is another wallet.
        let phrase = bip39::Mnemonic::parse(
            "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about",
        )
        .unwrap();
        let other =
            crate::wallet::encode_account(&crate::wallet::account_from_phrase(&phrase, bitcoin::Network::Regtest));
        s.env.insert("DEADHOP_XPUB", other);
        let e = App::start(load(&s.env).unwrap()).await.err().unwrap().to_string();
        assert!(e.contains("different wallet key"), "{e}");
    }

    #[test]
    fn a_blank_log_filter_means_the_default() {
        assert_eq!(log_filter(None), DEFAULT_LOG_FILTER);
        assert_eq!(log_filter(Some(" ".into())), DEFAULT_LOG_FILTER);
        assert_eq!(log_filter(Some("debug".into())), "debug");
    }

    #[test]
    fn the_public_key_is_printed_and_stable() {
        let tmp = tempfile::tempdir().unwrap();
        let data = tmp.path().join("data");
        let mut first = Vec::new();
        assert!(pubkey(&data, &mut first) == ExitCode::SUCCESS);
        let mut second = Vec::new();
        assert!(pubkey(&data, &mut second) == ExitCode::SUCCESS);
        let text = String::from_utf8(first.clone()).unwrap();
        assert_eq!(first, second, "the identity is created once");
        assert_eq!(text.trim().len(), 64);
        let keys = load_or_create_keys(&data.join(KEY_FILE)).unwrap();
        assert_eq!(text.trim(), keys.public_key().to_hex());
        assert_eq!(fs::metadata(&data).unwrap().permissions().mode() & 0o777, 0o700);
        // A data directory that cannot be created is reported.
        let file = tmp.path().join("file");
        fs::write(&file, "x").unwrap();
        let mut out = Vec::new();
        assert!(pubkey(&file.join("data"), &mut out) == ExitCode::FAILURE);
        assert!(out.is_empty());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn the_service_runs_against_a_pinned_tls_backend() {
        let mut s = stack().await;
        let (tls, fp) = FakeElectrum::start_tls().await;
        s.env.insert("DEADHOP_ELECTRUM", format!("tls://{}", tls.addr));
        s.env.insert("DEADHOP_ELECTRUM_CERT_SHA256", fp);
        let app = App::start(load(&s.env).unwrap()).await.unwrap();
        let admin = connect(&s, &app.pubkey, s.admin.clone()).await;
        let h = health_until(&admin, |h| h["chain"]["ok"] == true).await;
        assert_eq!(h["chain"]["transport"], "tls-pinned");
        app.stop().await;
        // A wrong pin keeps the service from starting at all.
        s.env.insert("DEADHOP_ELECTRUM_CERT_SHA256", "00".repeat(32));
        let e = App::start(load(&s.env).unwrap()).await.err().unwrap().to_string();
        assert!(e.contains("pinned fingerprint"), "{e}");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_restart_that_cannot_reconnect_stops_the_process() {
        let s = stack().await;
        let app = App::start(load(&s.env).unwrap()).await.unwrap();
        // The new list cannot start and going back fails too (here: the
        // database refuses the revert). The process must exit for its
        // supervisor rather than stay up unreachable.
        let mut broken = app.svc.settings().clone();
        broken.relays = vec!["unusable".into()];
        app.svc.apply_settings(broken);
        app.svc.db.execute_raw("DROP TABLE meta");
        app.svc.schedule_restart();
        let e = app
            .until(tokio::time::sleep(Duration::from_secs(20)))
            .await
            .unwrap_err();
        assert!(e.to_string().contains("stopped unexpectedly"), "{e}");
        app.stop().await;
    }

    /// Wait until `f` holds, polling.
    async fn eventually(what: &str, limit: Duration, f: impl Fn() -> bool) {
        let wait = async {
            while !f() {
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        };
        let done = tokio::time::timeout(limit, wait).await;
        assert!(done.is_ok(), "timed out waiting for {what}");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn relays_that_die_after_the_check_are_replaced_by_the_environments() {
        // Issue #6: a new relay list passes the 30-second check, then dies.
        // Every remote fix would travel over those dead relays, so the
        // supervisor goes back to the environment's relays by itself.
        let s = stack().await;
        let second = MockRelay::run().await.unwrap();
        let second_url = second.url().await.to_string();
        let app = App::start(load(&s.env).unwrap()).await.unwrap();
        let mut moved = app.svc.settings().clone();
        moved.relays = vec![second_url.clone()];
        app.svc.apply_settings(moved);
        app.svc.schedule_restart();
        // Let the new list pass its check before the relay goes away.
        tokio::time::sleep(RESTART_GRACE + Duration::from_secs(1)).await;
        assert_eq!(app.svc.settings().relays, vec![second_url]);
        second.shutdown();

        let env_relays = vec![s.relay_url.clone()];
        eventually("the fallback", Duration::from_secs(20), || {
            app.svc.settings().relays == env_relays
        })
        .await;
        let last = app.svc.db.audit_recent(1).unwrap().remove(0);
        assert_eq!(
            (last.action.as_str(), last.detail.as_str()),
            ("revert_connection", DEAD_RELAYS)
        );
        // Reachable again over the environment's relay.
        let admin = connect(&s, &app.pubkey, s.admin.clone()).await;
        assert_eq!(call(&admin, "get_info", json!({})).await.unwrap()["is_admin"], true);
        app.stop().await;
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_fallback_that_cannot_be_recorded_stops_the_process() {
        let s = stack().await;
        let second = MockRelay::run().await.unwrap();
        let app = App::start(load(&s.env).unwrap()).await.unwrap();
        let mut moved = app.svc.settings().clone();
        moved.relays = vec![second.url().await.to_string()];
        app.svc.apply_settings(moved);
        app.svc.schedule_restart();
        tokio::time::sleep(RESTART_GRACE + Duration::from_secs(1)).await;
        second.shutdown();
        app.svc.db.execute_raw("DROP TABLE meta");
        let e = app
            .until(tokio::time::sleep(Duration::from_secs(20)))
            .await
            .unwrap_err();
        assert!(e.to_string().contains("stopped unexpectedly"), "{e}");
        app.stop().await;
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn dead_environment_relays_are_only_retried() {
        // With nothing better to go back to, the service stays up and the
        // relay pool keeps retrying.
        let s = stack().await;
        let app = App::start(load(&s.env).unwrap()).await.unwrap();
        s._relay.shutdown();
        // Long enough for the watchdog to give up on them at least once, with
        // room for a slow (instrumented, parallel) test run.
        tokio::time::sleep(RELAY_DEAD_AFTER * 2 + Duration::from_secs(2)).await;
        assert!(!app.failed.is_cancelled());
        assert_eq!(app.svc.settings().relays, vec![s.relay_url.clone()]);
        app.stop().await;
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn saved_relays_that_are_dead_at_startup_give_way_to_the_environments() {
        // Restarting the host is a way back too: saved relays that do not
        // connect at startup are replaced by the environment's.
        let s = stack().await;
        let first = App::start(load(&s.env).unwrap()).await.unwrap();
        first
            .svc
            .revert_connection(vec!["ws://127.0.0.1:9".into()], "test: save a dead relay")
            .unwrap();
        first.stop().await;

        let app = App::start(load(&s.env).unwrap()).await.unwrap();
        assert_eq!(app.svc.settings().relays, vec![s.relay_url.clone()]);
        let last = app.svc.db.audit_recent(1).unwrap().remove(0);
        assert_eq!(last.detail, DEAD_AT_START);
        let admin = connect(&s, &app.pubkey, s.admin.clone()).await;
        assert_eq!(call(&admin, "get_info", json!({})).await.unwrap()["is_admin"], true);
        app.stop().await;
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn survives_a_stray_first_message() {
        let s = stack().await;
        let app = App::start(load(&s.env).unwrap()).await.unwrap();

        let mut attacker = NostrClientTransport::new(
            Keys::generate(),
            NostrClientTransportConfig::default()
                .with_relay_urls(vec![s.relay_url.clone()])
                .with_server_pubkey(app.pubkey.clone())
                .with_encryption_mode(EncryptionMode::Required),
        )
        .await
        .unwrap();
        attacker.start().await.unwrap();
        for method in ["notifications/initialized", "notifications/cancelled"] {
            let stray = contextvm_sdk::JsonRpcMessage::Notification(contextvm_sdk::JsonRpcNotification {
                jsonrpc: "2.0".into(),
                method: method.into(),
                params: None,
            });
            attacker.send(&stray).await.unwrap();
        }
        tokio::time::sleep(Duration::from_millis(500)).await;

        let honest = tokio::time::timeout(Duration::from_secs(10), async {
            let admin = connect(&s, &app.pubkey, s.admin.clone()).await;
            call(&admin, "get_info", json!({})).await.unwrap()
        })
        .await
        .expect("the service must still answer after a stray first message");
        assert_eq!(honest["is_admin"], true);
        app.stop().await;
    }

    #[tokio::test]
    async fn request_loop_reports_why_it_ended() {
        use crate::handshake::tests::scripted;
        let r = rig(|_| {}).await;

        // Transport closes after a completed handshake.
        let (transport, (tx, sent, _)) = scripted();
        drop(tx);
        let ended = serve(
            PayServer::new(r.svc.clone()),
            LocalHandshake::new(transport),
            CancellationToken::new(),
        )
        .await;
        assert!(
            ended.unwrap().contains("Closed"),
            "serving ended because the transport closed"
        );
        assert!(
            sent.lock().unwrap().is_empty(),
            "the local handshake reply never reaches the network"
        );

        // Without the local handshake, a transport that closes first cannot even start.
        let (transport, (tx, _, _)) = scripted();
        drop(tx);
        assert!(
            serve(PayServer::new(r.svc.clone()), transport, CancellationToken::new())
                .await
                .is_err()
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn an_ended_request_loop_stops_the_process() {
        let s = stack().await;
        let app = App::start(load(&s.env).unwrap()).await.unwrap();
        // Healthy: waiting ends only when shutdown is requested.
        app.until(tokio::time::sleep(Duration::from_millis(50))).await.unwrap();
        // The loop ends on its own: that is reported as a failure.
        app.current.lock().unwrap().cancel();
        let e = app.until(std::future::pending()).await.unwrap_err();
        assert!(e.to_string().contains("stopped unexpectedly"), "{e}");
        app.stop().await;
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn start_fails_fast_on_misconfiguration() {
        let s = stack().await;
        let start = |env: HashMap<&'static str, String>| async move {
            App::start(load(&env).unwrap())
                .await
                .err()
                .expect("startup must fail")
                .to_string()
        };

        // Electrum server on another chain.
        s.fake.set(|st| st.genesis = "00".repeat(32));
        assert!(start(s.env.clone()).await.contains("different network"));
        s.fake
            .set(|st| st.genesis = crate::electrum::testutil::REGTEST_GENESIS.into());

        // Electrum server unreachable.
        let mut env = s.env.clone();
        env.insert("DEADHOP_ELECTRUM", "127.0.0.1:1".into());
        assert!(start(env).await.contains("chain backend"));

        // Not an extended public key.
        let mut env = s.env.clone();
        env.insert("DEADHOP_XPUB", "nope".into());
        assert!(start(env).await.contains("DEADHOP_XPUB"));

        // Data directory cannot be created.
        let mut env = s.env.clone();
        let file = s.dir.path().join("file");
        fs::write(&file, "x").unwrap();
        env.insert("DEADHOP_DATA_DIR", file.join("sub").to_string_lossy().into_owned());
        assert!(start(env).await.contains("io:"));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn run_and_cli_serve_until_shutdown() {
        let s = stack().await;
        run(load(&s.env).unwrap(), tokio::time::sleep(Duration::from_millis(200)))
            .await
            .unwrap();

        let env = s.env.clone();
        let code = cli(
            move |k| env.get(k).cloned(),
            tokio::time::sleep(Duration::from_millis(200)),
        )
        .await;
        assert_eq!(format!("{code:?}"), format!("{:?}", ExitCode::SUCCESS));

        let code = cli(|_| None, std::future::ready(())).await;
        assert_eq!(format!("{code:?}"), format!("{:?}", ExitCode::FAILURE));

        // A startup failure after configuration parsing is also a failure exit.
        let mut env = s.env.clone();
        env.insert("DEADHOP_XPUB", "nope".into());
        let code = cli(move |k| env.get(k).cloned(), std::future::ready(())).await;
        assert_eq!(format!("{code:?}"), format!("{:?}", ExitCode::FAILURE));
    }
}
