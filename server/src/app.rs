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
use crate::mcp::PayServer;
use crate::relay::{parse_proxy, Pool};
use crate::service::{system_clock, RelayProbe, RelayState, Service};
use crate::wallet::Wallet;

/// File (inside the data directory) holding the server's nostr secret key.
pub const KEY_FILE: &str = "server.nsec";
/// File the server writes its public key to, for operators and tooling.
pub const PUBKEY_FILE: &str = "server.pubkey";
/// Database file name.
pub const DB_FILE: &str = "cvmpay.sqlite";
/// Longest wait for one exchange with the Electrum server.
const ELECTRUM_TIMEOUT: Duration = Duration::from_secs(20);

/// Load the server identity, creating it on first start. The file must be
/// private to the owner; a key readable by others is refused.
pub fn load_or_create_keys(path: &Path) -> Result<Keys> {
    match fs::File::open(path) {
        Ok(mut file) => {
            // Permissions are read from the open handle, so the file that is
            // checked is the file that is read.
            if file.metadata()?.permissions().mode() & 0o077 != 0 {
                return Err(Error::Config(format!(
                    "{} is accessible by other users; run: chmod 600 {}",
                    path.display(),
                    path.display()
                )));
            }
            let mut text = String::new();
            file.read_to_string(&mut text)?;
            Keys::parse(text.trim())
                .map_err(|_| Error::Config(format!("{} does not contain a nostr secret key", path.display())))
        }
        Err(e) if e.kind() == ErrorKind::NotFound => {
            let keys = Keys::generate();
            let nsec = keys
                .secret_key()
                .to_bech32()
                .expect("a valid secret key always encodes as bech32");
            let mut file = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(path)?;
            file.write_all(nsec.as_bytes())?;
            // The identity must survive a crash right after first start.
            file.sync_all()?;
            Ok(keys)
        }
        Err(e) => Err(e.into()),
    }
}

/// Create the data directory (owner-only) if needed and load the server
/// identity from it, creating that on first start too.
pub fn open_identity(data_dir: &Path) -> Result<Keys> {
    fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(data_dir)
        .and_then(|()| fs::set_permissions(data_dir, fs::Permissions::from_mode(0o700)))
        .map_err(Error::from)
        .and_then(|()| load_or_create_keys(&data_dir.join(KEY_FILE)))
}

/// The data directory: `CVMPAY_DATA_DIR`, or `data` when unset.
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
            eprintln!("cvmpay: {e}");
            ExitCode::FAILURE
        }
    }
}

/// Poll the chain forever, at the live `poll_secs` setting unless `every` is given.
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
        let pool = Arc::new(Pool::new(keys.clone(), parse_proxy(&proxy)?));
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

/// Bring the request loop back up with the live relays and proxy. If they
/// cannot connect, go back to the previous ones so the service stays reachable.
async fn relink(keys: &Keys, cfg: &Config, svc: &Arc<Service>, previous: (Vec<String>, String)) -> Result<Link> {
    let wanted = {
        let s = svc.settings();
        (s.relays.clone(), s.proxy.clone())
    };
    if wanted == previous {
        return Link::up(keys, cfg, wanted.0, wanted.1, svc).await;
    }
    match Link::up(keys, cfg, wanted.0, wanted.1, svc).await {
        Ok(link) if connects_within(&link, RELAY_CHECK).await => return Ok(link),
        Ok(link) => link.close().await,
        Err(e) => tracing::warn!(error = %e, "could not start the new relay connections"),
    }
    tracing::warn!("no relay connected with the new settings; going back to the previous ones");
    svc.revert_connection(
        previous.0.clone(),
        previous.1.clone(),
        "no relay connected with the new relays or proxy",
    )?;
    Link::up(keys, cfg, previous.0, previous.1, svc).await
}

/// Keep the request loop running, restarting it when asked.
async fn supervise(
    svc: Arc<Service>,
    keys: Keys,
    cfg: Config,
    mut link: Link,
    stop: CancellationToken,
    failed: CancellationToken,
    current: Arc<std::sync::Mutex<CancellationToken>>,
) {
    loop {
        *current.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = link.cancel.clone();
        tokio::select! {
            _ = stop.cancelled() => {
                link.close().await;
                return;
            }
            _ = link.ended.cancelled() => {
                tracing::warn!("request loop stopped unexpectedly");
                failed.cancel();
                return;
            }
            _ = svc.restart_requested() => {
                tokio::time::sleep(RESTART_GRACE).await;
                let previous = (link.relays.clone(), link.proxy.clone());
                link.close().await;
                link = match relink(&keys, &cfg, &svc, previous).await {
                    Ok(l) => l,
                    Err(e) => {
                        tracing::error!(error = %e, "could not restart the relay connections");
                        failed.cancel();
                        return;
                    }
                };
                svc.restarted();
                tracing::info!(relays = link.relays.len(), "relay connections restarted");
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
                "the Electrum server is on a different network than CVMPAY_NETWORK".into(),
            ));
        }
        let chain = Arc::new(chain.expect_genesis(expected));

        let db = Db::open(&cfg.data_dir.join(DB_FILE))?;
        let svc = Service::new(cfg.clone(), db, wallet, chain, system_clock(), pubkey.clone());
        let svc = Arc::new(svc?);
        let warnings = svc
            .settings()
            .warnings(cfg.network, &cfg.electrum, &cfg.electrum_security);
        warnings.iter().for_each(|w| tracing::warn!("{w}"));

        let (relays, proxy) = {
            let s = svc.settings();
            (s.relays.clone(), s.proxy.clone())
        };
        let via_proxy = !proxy.is_empty();
        let link = Link::up(&keys, &cfg, relays.clone(), proxy, &svc).await?;
        let stop = CancellationToken::new();
        let failed = CancellationToken::new();
        let current = Arc::new(std::sync::Mutex::new(link.cancel.clone()));
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
        tracing::info!(%pubkey, network = %cfg.network, relays = relays.len(), via_proxy, paused = svc.is_paused(), "cvmpay is serving");
        tracing::info!(address = %svc.first_address()?, "first receive address (index 0): check that your wallet lists it");
        Ok(App {
            svc,
            pubkey,
            stop,
            failed,
            supervisor,
            monitor,
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
        self.stop.cancel();
        let _ = self.supervisor.await;
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
/// `CVMPAY_LOG`.
pub const DEFAULT_LOG_FILTER: &str = "warn,cvmpay=info,contextvm_sdk=off,rmcp=off";

/// The log filter: `CVMPAY_LOG`, or the default when it is unset or blank
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
  cvmpay                          run the service (configured by CVMPAY_* variables)
  cvmpay fingerprint HOST:PORT    print the SHA-256 fingerprint of an Electrum server's TLS certificate
  cvmpay pubkey                   print the service's public key (hex), creating its identity in CVMPAY_DATA_DIR if needed";

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
                "TLS certificate of {addr}, SHA-256:\n  {fp}\n\nTo pin it:\n  CVMPAY_ELECTRUM=tls://{addr}\n  CVMPAY_ELECTRUM_CERT_SHA256={fp}\n\n\
                 Check it against the certificate on the server first, for example:\n  \
                 openssl x509 -in <fulcrum cert.pem> -noout -fingerprint -sha256\n"
            );
            let _ = out.write_all(text.as_bytes());
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("cvmpay: cannot read a TLS certificate from {addr}: {e}");
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
        [cmd] if cmd == "pubkey" => pubkey(&data_dir(lookup("CVMPAY_DATA_DIR")), &mut stdout()),
        _ => {
            eprintln!("{USAGE}");
            ExitCode::from(2)
        }
    }
}

/// Entry point shared by the binary and the tests.
pub async fn cli(lookup: impl Fn(&str) -> Option<String>, shutdown: impl Future<Output = ()>) -> ExitCode {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::new(log_filter(lookup("CVMPAY_LOG"))))
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
            eprintln!("cvmpay: {e}");
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
        env.insert("CVMPAY_RELAYS", relay_url.clone());
        env.insert("CVMPAY_ELECTRUM", fake.addr.clone());
        env.insert(
            "CVMPAY_DATA_DIR",
            dir.path().join("data").to_string_lossy().into_owned(),
        );
        env.insert("CVMPAY_ADMINS", admin.public_key().to_hex());
        env.insert("CVMPAY_PUBLIC_INVOICING", "true".into());
        env.insert("CVMPAY_POW_BITS", "8".into());
        env.insert("CVMPAY_POLL_SECS", "1".into());
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
                "control_service",
                "create_invoice",
                "get_audit_log",
                "get_health",
                "get_info",
                "get_invoice",
                "get_metrics",
                "get_payment_series",
                "get_settings",
                "get_stats",
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
        let b = call(&admin, "backup_database", json!({})).await.unwrap();
        assert!(s
            .dir
            .path()
            .join("data/backups")
            .join(b["file"].as_str().unwrap())
            .exists());

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
            .insert("CVMPAY_PROXY", format!("socks5://svc:pw@127.0.0.1:{port}"));
        let app = App::start(load(&s.env).unwrap()).await.unwrap();
        // The operator connects directly; the service only through the proxy.
        let admin = connect(&s, &app.pubkey, s.admin.clone()).await;
        let h = health_until(&admin, |h| h["relays"][0]["connected"] == true).await;
        assert_eq!(h["proxy"], format!("socks5://svc:***@127.0.0.1:{port}"));
        let v = call(&admin, "get_settings", json!({})).await.unwrap();
        assert!(!v["settings"]["proxy"].as_str().unwrap().contains("pw"));

        // A proxy that is not there is reverted to the one that worked.
        call(
            &admin,
            "update_settings",
            json!({ "patch": { "proxy": "socks5://127.0.0.1:9" } }),
        )
        .await
        .unwrap();
        let h = health_until(&admin, |h| h["service"]["restarts"] == 1).await;
        assert_eq!(h["proxy"], format!("socks5://svc:***@127.0.0.1:{port}"));
        // Turning the proxy off is a change of connection too.
        call(&admin, "update_settings", json!({ "patch": { "proxy": "" } }))
            .await
            .unwrap();
        let h = health_until(&admin, |h| h["service"]["restarts"] == 2).await;
        assert_eq!(h["proxy"], "");
        app.stop().await;
    }

    #[tokio::test]
    async fn the_fingerprint_command_prints_what_to_pin() {
        let (fake, fp) = FakeElectrum::start_tls().await;
        let mut out = Vec::new();
        let code = fingerprint(&format!("tls://{}", fake.addr), &mut out).await;
        assert_eq!(format!("{code:?}"), format!("{:?}", ExitCode::SUCCESS));
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains(&format!("CVMPAY_ELECTRUM_CERT_SHA256={fp}")), "{text}");
        assert!(text.contains(&format!("CVMPAY_ELECTRUM=tls://{}", fake.addr)));
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
        let dead = std::net::TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .to_string();
        let fp = ["fingerprint".to_string(), dead];
        assert!(command(&fp, none, std::future::ready(())).await == ExitCode::FAILURE);
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("data").to_string_lossy().into_owned();
        let lookup = |k: &str| (k == "CVMPAY_DATA_DIR").then(|| dir.clone());
        let pk = ["pubkey".to_string()];
        assert!(command(&pk, lookup, std::future::ready(())).await == ExitCode::SUCCESS);
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
        s.env.insert("CVMPAY_ELECTRUM", format!("tls://{}", tls.addr));
        s.env.insert("CVMPAY_ELECTRUM_CERT_SHA256", fp);
        let app = App::start(load(&s.env).unwrap()).await.unwrap();
        let admin = connect(&s, &app.pubkey, s.admin.clone()).await;
        let h = health_until(&admin, |h| h["chain"]["ok"] == true).await;
        assert_eq!(h["chain"]["transport"], "tls-pinned");
        app.stop().await;
        // A wrong pin keeps the service from starting at all.
        s.env.insert("CVMPAY_ELECTRUM_CERT_SHA256", "00".repeat(32));
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
        broken.proxy = "unusable".into();
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
        env.insert("CVMPAY_ELECTRUM", "127.0.0.1:1".into());
        assert!(start(env).await.contains("chain backend"));

        // Not an extended public key.
        let mut env = s.env.clone();
        env.insert("CVMPAY_XPUB", "nope".into());
        assert!(start(env).await.contains("CVMPAY_XPUB"));

        // Data directory cannot be created.
        let mut env = s.env.clone();
        let file = s.dir.path().join("file");
        fs::write(&file, "x").unwrap();
        env.insert("CVMPAY_DATA_DIR", file.join("sub").to_string_lossy().into_owned());
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
        env.insert("CVMPAY_XPUB", "nope".into());
        let code = cli(move |k| env.get(k).cloned(), std::future::ready(())).await;
        assert_eq!(format!("{code:?}"), format!("{:?}", ExitCode::FAILURE));
    }
}
