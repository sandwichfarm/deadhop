//! Chain backend speaking the Electrum protocol (Fulcrum, electrs, ElectrumX).
//!
//! The service only ever asks "what was paid to this script?", so the client is
//! a few dozen lines of newline-delimited JSON-RPC over one connection: plain
//! TCP, TLS checked against the public roots, or TLS with a pinned certificate.
//! Every response is treated as hostile input: lines are size-capped, shapes
//! are checked, and each transaction is re-hashed before its outputs are
//! trusted to belong to the txid the server claimed.

use std::collections::{HashMap, HashSet};
use std::time::Duration;

use async_trait::async_trait;
use bitcoin::consensus::encode::deserialize;
use bitcoin::{ScriptBuf, Transaction};
use serde_json::{json, Value};
use std::sync::Arc;

use bitcoin::hashes::{sha256, Hash};
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::crypto::WebPkiSupportedAlgorithms;
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::{DigitallySignedStruct, SignatureScheme};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader, ReadHalf, WriteHalf};
use tokio::net::TcpStream;
use tokio::sync::Mutex;

use crate::relay::Io;

use crate::error::{Error, Result};
use crate::wallet::scripthash;

/// Largest response line accepted from the server.
pub const MAX_LINE_BYTES: u64 = 4 * 1024 * 1024;
/// Most history entries accepted for one invoice address.
pub const MAX_HISTORY: usize = 200;
/// Entries kept in the per-transaction value cache before it is reset.
const CACHE_LIMIT: usize = 50_000;

/// One payment to a watched script.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Payment {
    /// Satoshis paid to the script by this transaction.
    pub value_sat: u64,
    /// Block height, or `None` while unconfirmed.
    pub height: Option<u32>,
}

/// Chain view of one script: its payments, or why they could not be read.
/// A problem with one script never hides the others.
pub type ScriptOutcome = std::result::Result<Vec<Payment>, String>;

/// Chain view for a set of scripts at one point in time.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Snapshot {
    /// Current best block height.
    pub tip: u32,
    /// Outcome per requested script, in request order.
    pub payments: Vec<ScriptOutcome>,
}

/// What the service needs from a chain backend.
#[async_trait]
pub trait Chain: Send + Sync {
    /// Genesis block hash (hex) of the chain the backend serves.
    async fn genesis_hash(&self) -> Result<String>;
    /// Payments received by each script.
    async fn snapshot(&self, scripts: &[ScriptBuf]) -> Result<Snapshot>;
}

fn chain_err(what: impl std::fmt::Display) -> Error {
    Error::Chain(what.to_string())
}

/// How the connection to the Electrum server is protected.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Security {
    /// Plain TCP: only for a server on this machine or a private network.
    Plain,
    /// TLS, with the certificate checked against the public web roots.
    Tls,
    /// TLS, accepting only certificates with one of these SHA-256 fingerprints
    /// (of the DER-encoded leaf certificate). Suits self-signed servers.
    Pinned(Vec<[u8; 32]>),
}

impl Security {
    /// Short name shown to operators: `tcp`, `tls` or `tls-pinned`.
    pub fn label(&self) -> &'static str {
        match self {
            Security::Plain => "tcp",
            Security::Tls => "tls",
            Security::Pinned(_) => "tls-pinned",
        }
    }
}

/// Parse a certificate fingerprint: 64 hex digits, colons allowed.
pub fn parse_fingerprint(s: &str) -> Option<[u8; 32]> {
    let hex: String = s.trim().chars().filter(|c| *c != ':').collect();
    let mut out = [0u8; 32];
    hex::decode_to_slice(hex, &mut out).ok()?;
    Some(out)
}

/// SHA-256 of a DER certificate, as lowercase hex.
pub fn fingerprint_hex(der: &[u8]) -> String {
    hex::encode(sha256::Hash::hash(der).to_byte_array())
}

/// Accepts exactly the pinned certificates, ignoring names and authorities,
/// but still makes the server prove it holds the certificate's private key.
/// Without pins it accepts any certificate and records it (only used to
/// read a server's fingerprint).
#[derive(Debug)]
struct PinnedCert {
    pins: Option<Vec<[u8; 32]>>,
    seen: std::sync::Mutex<Option<[u8; 32]>>,
    algs: WebPkiSupportedAlgorithms,
}

impl ServerCertVerifier for PinnedCert {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> std::result::Result<ServerCertVerified, rustls::Error> {
        let fp = sha256::Hash::hash(end_entity.as_ref()).to_byte_array();
        *self.seen.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = Some(fp);
        match &self.pins {
            Some(pins) if !pins.contains(&fp) => Err(rustls::Error::General(format!(
                "certificate {} does not match the pinned fingerprint",
                hex::encode(fp)
            ))),
            _ => Ok(ServerCertVerified::assertion()),
        }
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> std::result::Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls12_signature(message, cert, dss, &self.algs)
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> std::result::Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(message, cert, dss, &self.algs)
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.algs.supported_schemes()
    }
}

fn host_of(addr: &str) -> &str {
    let host = addr.rsplit_once(':').map_or(addr, |(h, _)| h);
    host.trim_start_matches('[').trim_end_matches(']')
}

fn tls_config(verifier: Option<Arc<PinnedCert>>) -> rustls::ClientConfig {
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let builder = rustls::ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .expect("the ring provider supports the default protocol versions");
    match verifier {
        Some(v) => builder
            .dangerous()
            .with_custom_certificate_verifier(v)
            .with_no_client_auth(),
        None => {
            let mut roots = rustls::RootCertStore::empty();
            roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
            builder.with_root_certificates(roots).with_no_client_auth()
        }
    }
}

fn pinned(pins: Option<Vec<[u8; 32]>>) -> Arc<PinnedCert> {
    Arc::new(PinnedCert {
        pins,
        seen: std::sync::Mutex::new(None),
        algs: rustls::crypto::ring::default_provider().signature_verification_algorithms,
    })
}

async fn tls_connect(
    tcp: TcpStream,
    addr: &str,
    config: rustls::ClientConfig,
) -> Result<tokio_rustls::client::TlsStream<TcpStream>> {
    let name = ServerName::try_from(host_of(addr).to_string()).map_err(chain_err)?;
    let tls = tokio_rustls::TlsConnector::from(Arc::new(config))
        .connect(name, tcp)
        .await;
    tls.map_err(|e| chain_err(format!("TLS: {e}")))
}

/// Connect to `addr` over TLS and return the SHA-256 fingerprint of the
/// certificate it presents, for pinning. Nothing else is exchanged.
pub async fn fetch_fingerprint(addr: &str, timeout: Duration) -> Result<String> {
    let work = async {
        let tcp = TcpStream::connect(addr).await.map_err(chain_err)?;
        let verifier = pinned(None);
        tls_connect(tcp, addr, tls_config(Some(verifier.clone()))).await?;
        let seen = *verifier.seen.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        Ok(hex::encode(
            seen.expect("a completed TLS handshake has seen a certificate"),
        ))
    };
    tokio::time::timeout(timeout, work)
        .await
        .unwrap_or(Err(chain_err("timed out")))
}

struct Conn {
    reader: BufReader<ReadHalf<Box<dyn Io>>>,
    writer: WriteHalf<Box<dyn Io>>,
    next_id: u64,
}

impl Conn {
    async fn open(addr: &str, security: &Security, expected_genesis: Option<&str>) -> Result<Conn> {
        let tcp = TcpStream::connect(addr).await.map_err(chain_err)?;
        let stream: Box<dyn Io> = match security {
            Security::Plain => Box::new(tcp),
            Security::Tls => Box::new(tls_connect(tcp, addr, tls_config(None)).await?),
            Security::Pinned(pins) => {
                Box::new(tls_connect(tcp, addr, tls_config(Some(pinned(Some(pins.clone()))))).await?)
            }
        };
        let (r, w) = tokio::io::split(stream);
        let mut conn = Conn {
            reader: BufReader::new(r),
            writer: w,
            next_id: 0,
        };
        conn.exchange(&[("server.version", json!(["deadhop", "1.4"]))]).await?;
        // Checked on every (re)connection, so the backend cannot be swapped
        // for one on another chain while the service is running.
        if let Some(want) = expected_genesis {
            let features = conn.exchange(&[("server.features", json!([]))]).await?;
            if features[0]["genesis_hash"]
                .as_str()
                .map(str::to_ascii_lowercase)
                .as_deref()
                != Some(want)
            {
                return Err(chain_err("server is on a different chain"));
            }
        }
        Ok(conn)
    }

    /// Send all requests, then collect one result per request.
    async fn exchange(&mut self, reqs: &[(&str, Value)]) -> Result<Vec<Value>> {
        let base = self.next_id;
        self.next_id += reqs.len() as u64;
        let mut out = Vec::new();
        for (i, (method, params)) in reqs.iter().enumerate() {
            let line = json!({"jsonrpc": "2.0", "id": base + i as u64, "method": method, "params": params});
            out.extend_from_slice(line.to_string().as_bytes());
            out.push(b'\n');
        }
        self.writer.write_all(&out).await.map_err(chain_err)?;

        let mut results: Vec<Option<Value>> = vec![None; reqs.len()];
        let mut missing = reqs.len();
        while missing > 0 {
            let mut buf = Vec::new();
            let n = (&mut self.reader)
                .take(MAX_LINE_BYTES)
                .read_until(b'\n', &mut buf)
                .await
                .map_err(chain_err)?;
            if n == 0 {
                return Err(chain_err("connection closed"));
            }
            if !buf.ends_with(b"\n") {
                return Err(chain_err("response truncated or too large"));
            }
            let mut msg: Value = serde_json::from_slice(&buf).map_err(|_| chain_err("malformed response"))?;
            // Lines without one of our ids are subscription notifications.
            let Some(slot) = msg
                .get("id")
                .and_then(Value::as_u64)
                .and_then(|id| id.checked_sub(base))
                .and_then(|i| results.get_mut(i as usize))
                .filter(|slot| slot.is_none())
            else {
                continue;
            };
            if msg.get("error").is_some_and(|e| !e.is_null()) {
                return Err(chain_err(format!("server error: {}", msg["error"])));
            }
            *slot = Some(msg["result"].take());
            missing -= 1;
        }
        Ok(results.into_iter().flatten().collect())
    }
}

type ValueCache = HashMap<(String, ScriptBuf), u64>;

/// Electrum protocol client.
pub struct Electrum {
    addr: String,
    security: Security,
    timeout: Duration,
    expected_genesis: Option<String>,
    conn: Mutex<Option<Conn>>,
    /// `(txid, script)` -> satoshis that transaction pays to the script.
    cache: Mutex<ValueCache>,
}

impl Electrum {
    /// Create a client for `host:port`. Connects lazily and reconnects after errors.
    pub fn new(addr: impl Into<String>, timeout: Duration) -> Self {
        Electrum {
            addr: addr.into(),
            security: Security::Plain,
            timeout,
            expected_genesis: None,
            conn: Mutex::new(None),
            cache: Mutex::new(HashMap::new()),
        }
    }

    /// Protect the connection with TLS (see [`Security`]).
    pub fn with_security(mut self, security: Security) -> Self {
        self.security = security;
        self
    }

    /// Refuse any server whose genesis block hash (hex) differs.
    pub fn expect_genesis(mut self, hash: impl Into<String>) -> Self {
        self.expected_genesis = Some(hash.into().to_ascii_lowercase());
        self
    }

    async fn batch(&self, reqs: &[(&str, Value)]) -> Result<Vec<Value>> {
        let mut slot = self.conn.lock().await;
        let work = async {
            let conn = match slot.as_mut() {
                Some(c) => c,
                None => slot.insert(Conn::open(&self.addr, &self.security, self.expected_genesis.as_deref()).await?),
            };
            conn.exchange(reqs).await
        };
        let res = match tokio::time::timeout(self.timeout, work).await {
            Ok(r) => r,
            Err(_) => Err(chain_err("timed out")),
        };
        if res.is_err() {
            // Never reuse a connection in an unknown state.
            *slot = None;
        }
        res
    }

    /// Turn one script's history into payments, fetching and verifying the
    /// transactions whose value to this script is not yet known.
    async fn resolve(
        &self,
        script: &ScriptBuf,
        history: &[(String, Option<u32>)],
        cache: &mut ValueCache,
        fetched: &mut HashMap<String, Transaction>,
    ) -> ScriptOutcome {
        let wanted: Vec<&String> = history
            .iter()
            .map(|(txid, _)| txid)
            .filter(|txid| !cache.contains_key(&((*txid).clone(), script.clone())) && !fetched.contains_key(*txid))
            .collect();
        if !wanted.is_empty() {
            let reqs: Vec<(&str, Value)> = wanted
                .iter()
                .map(|t| ("blockchain.transaction.get", json!([t])))
                .collect();
            let raws = self.batch(&reqs).await.map_err(|e| e.to_string())?;
            for (txid, raw) in wanted.iter().zip(&raws) {
                let bytes = raw
                    .as_str()
                    .and_then(|h| hex::decode(h).ok())
                    .ok_or("bad transaction hex")?;
                let tx: Transaction = deserialize(&bytes).map_err(|_| "undecodable transaction")?;
                // The txid commits to the outputs, so a matching hash means the
                // outputs are the ones the history entry refers to.
                if tx.compute_txid().to_string() != **txid {
                    return Err("transaction does not match its txid".into());
                }
                fetched.insert((*txid).clone(), tx);
            }
        }

        let mut payments = Vec::new();
        for (txid, height) in history {
            let key = (txid.clone(), script.clone());
            let value_sat = match cache.get(&key) {
                Some(v) => *v,
                None => {
                    let paid = fetched[txid]
                        .output
                        .iter()
                        .filter(|o| o.script_pubkey == *script)
                        .fold(0u64, |sum, o| sum.saturating_add(o.value.to_sat()));
                    cache.insert(key, paid);
                    paid
                }
            };
            // A zero value means the transaction only spends from the script.
            if value_sat > 0 {
                payments.push(Payment {
                    value_sat,
                    height: *height,
                });
            }
        }
        Ok(payments)
    }
}

fn is_txid(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit())
}

#[async_trait]
impl Chain for Electrum {
    async fn genesis_hash(&self) -> Result<String> {
        let res = self.batch(&[("server.features", json!([]))]).await?;
        res[0]["genesis_hash"]
            .as_str()
            .map(str::to_ascii_lowercase)
            .ok_or_else(|| chain_err("server.features lacks genesis_hash"))
    }

    async fn snapshot(&self, scripts: &[ScriptBuf]) -> Result<Snapshot> {
        let mut reqs: Vec<(&str, Value)> = vec![("blockchain.headers.subscribe", json!([]))];
        for s in scripts {
            reqs.push(("blockchain.scripthash.get_history", json!([scripthash(s)])));
        }
        let res = self.batch(&reqs).await?;
        let tip = res[0]["height"]
            .as_u64()
            .and_then(|h| u32::try_from(h).ok())
            .ok_or_else(|| chain_err("bad tip height"))?;

        // Parse histories into unique (txid, height) pairs per script. A
        // malformed reply means the server is broken; an over-long history is
        // a property of that one script (anyone can send dust to an address).
        let mut histories: Vec<std::result::Result<Vec<(String, Option<u32>)>, String>> = Vec::new();
        for h in &res[1..] {
            let entries = h.as_array().ok_or_else(|| chain_err("bad history"))?;
            if entries.len() > MAX_HISTORY {
                histories.push(Err(format!("more than {MAX_HISTORY} transactions")));
                continue;
            }
            let mut seen = HashSet::new();
            let mut parsed = Vec::with_capacity(entries.len());
            for e in entries {
                let txid = e["tx_hash"]
                    .as_str()
                    .filter(|t| is_txid(t))
                    .ok_or_else(|| chain_err("bad tx_hash"))?;
                let height = e["height"].as_i64().ok_or_else(|| chain_err("bad tx height"))?;
                // Electrum reports 0 or -1 for mempool transactions.
                let height = u32::try_from(height).ok().filter(|h| *h > 0);
                let txid = txid.to_ascii_lowercase();
                // A transaction listed twice must not be counted twice.
                if seen.insert(txid.clone()) {
                    parsed.push((txid, height));
                }
            }
            histories.push(Ok(parsed));
        }

        let mut cache = self.cache.lock().await;
        if cache.len() > CACHE_LIMIT {
            cache.clear();
        }
        let mut fetched = HashMap::new();
        let mut payments = Vec::with_capacity(scripts.len());
        for (script, history) in scripts.iter().zip(histories) {
            payments.push(match history {
                Ok(history) => self.resolve(script, &history, &mut cache, &mut fetched).await,
                Err(why) => Err(why),
            });
        }
        Ok(Snapshot { tip, payments })
    }
}

#[cfg(test)]
pub(crate) mod testutil {
    //! Scriptable in-process Electrum server for hermetic tests.
    use super::*;
    use bitcoin::consensus::encode::serialize_hex;
    use bitcoin::{absolute, transaction, Amount, OutPoint, Sequence, TxIn, TxOut, Txid, Witness};
    use std::sync::{Arc, Mutex as StdMutex};
    use tokio::net::TcpListener;

    pub const REGTEST_GENESIS: &str = "0f9188f13cb7b2c71f2a335e3a4fc328bf5beb436012afca590b1a11466e2206";

    #[derive(Default)]
    pub struct State {
        pub tip: u32,
        pub genesis: String,
        /// scripthash -> [(txid, height)]
        pub history: HashMap<String, Vec<(String, i64)>>,
        /// txid -> raw hex
        pub txs: HashMap<String, String>,
        /// method -> full replacement for the `result` value
        pub result_override: HashMap<String, Value>,
        /// method -> raw bytes written instead of a response
        pub raw_override: HashMap<String, Vec<u8>>,
        /// method -> respond with a JSON-RPC error
        pub error_on: HashSet<String>,
        /// method -> never respond
        pub hang_on: HashSet<String>,
        /// method -> drop the connection
        pub close_on: HashSet<String>,
        /// Send an unrelated notification line before each response.
        pub noisy: bool,
        /// Every method name received, in order.
        pub calls: Vec<String>,
        /// Number of accepted TCP connections.
        pub connections: usize,
        counter: u8,
    }

    #[derive(Clone)]
    pub struct FakeElectrum {
        pub addr: String,
        pub state: Arc<StdMutex<State>>,
    }

    impl FakeElectrum {
        pub async fn start() -> Self {
            Self::start_with(None).await
        }

        /// The same server behind TLS with the test certificate. Returns it
        /// with the certificate's SHA-256 fingerprint.
        pub async fn start_tls() -> (Self, String) {
            Self::start_tls_versions(rustls::DEFAULT_VERSIONS).await
        }

        /// Like [`start_tls`](Self::start_tls), but speaking only TLS 1.2.
        pub async fn start_tls12() -> (Self, String) {
            Self::start_tls_versions(&[&rustls::version::TLS12]).await
        }

        async fn start_tls_versions(versions: &[&'static rustls::SupportedProtocolVersion]) -> (Self, String) {
            use rustls::pki_types::pem::PemObject;
            let cert = CertificateDer::from_pem_slice(CERT).unwrap();
            let fp = fingerprint_hex(cert.as_ref());
            let key = rustls::pki_types::PrivateKeyDer::from_pem_slice(KEY).unwrap();
            let config =
                rustls::ServerConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
                    .with_protocol_versions(versions)
                    .unwrap()
                    .with_no_client_auth()
                    .with_single_cert(vec![cert], key)
                    .unwrap();
            (
                Self::start_with(Some(tokio_rustls::TlsAcceptor::from(Arc::new(config)))).await,
                fp,
            )
        }

        async fn start_with(tls: Option<tokio_rustls::TlsAcceptor>) -> Self {
            let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
            let addr = listener.local_addr().unwrap().to_string();
            let state = Arc::new(StdMutex::new(State {
                tip: 100,
                genesis: REGTEST_GENESIS.to_string(),
                ..State::default()
            }));
            let shared = state.clone();
            tokio::spawn(async move {
                loop {
                    let (stream, _) = listener.accept().await.unwrap();
                    shared.lock().unwrap().connections += 1;
                    let (shared, tls) = (shared.clone(), tls.clone());
                    tokio::spawn(async move {
                        match tls {
                            None => serve(stream, shared).await,
                            Some(acceptor) => {
                                if let Ok(s) = acceptor.accept(stream).await {
                                    serve(s, shared).await;
                                }
                            }
                        }
                    });
                }
            });
            FakeElectrum { addr, state }
        }

        pub fn client(&self) -> Electrum {
            Electrum::new(self.addr.clone(), Duration::from_millis(500))
        }

        pub fn set<R>(&self, f: impl FnOnce(&mut State) -> R) -> R {
            f(&mut self.state.lock().unwrap())
        }

        /// Register a transaction paying `value_sat` to `script` at `height`
        /// (0 = mempool). Returns its txid.
        pub fn pay(&self, script: &ScriptBuf, value_sat: u64, height: i64) -> String {
            self.set(|s| {
                s.counter += 1;
                let tx = Transaction {
                    version: transaction::Version::TWO,
                    lock_time: absolute::LockTime::ZERO,
                    input: vec![TxIn {
                        previous_output: OutPoint {
                            txid: Txid::from_byte_array([s.counter; 32]),
                            vout: 0,
                        },
                        script_sig: ScriptBuf::new(),
                        sequence: Sequence::MAX,
                        witness: Witness::new(),
                    }],
                    output: vec![
                        TxOut {
                            value: Amount::from_sat(value_sat),
                            script_pubkey: script.clone(),
                        },
                        TxOut {
                            value: Amount::from_sat(777),
                            script_pubkey: ScriptBuf::new(),
                        },
                    ],
                };
                let txid = tx.compute_txid().to_string();
                s.txs.insert(txid.clone(), serialize_hex(&tx));
                s.history
                    .entry(scripthash(script))
                    .or_default()
                    .push((txid.clone(), height));
                txid
            })
        }

        /// Change the height of an already registered transaction.
        pub fn set_height(&self, script: &ScriptBuf, txid: &str, height: i64) {
            self.set(|s| {
                for e in s.history.get_mut(&scripthash(script)).unwrap() {
                    if e.0 == txid {
                        e.1 = height;
                    }
                }
            })
        }
    }

    const CERT: &[u8] = include_bytes!("../tests/fixtures/proxy-test-cert.pem");
    const KEY: &[u8] = include_bytes!("../tests/fixtures/proxy-test-key.pem");

    async fn serve<S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Send + 'static>(
        stream: S,
        state: Arc<StdMutex<State>>,
    ) {
        let (r, mut w) = tokio::io::split(stream);
        let mut lines = BufReader::new(r).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            let req: Value = serde_json::from_str(&line).unwrap();
            let method = req["method"].as_str().unwrap().to_string();
            let reply: Option<Vec<u8>> = {
                let mut s = state.lock().unwrap();
                s.calls.push(method.clone());
                if s.close_on.contains(&method) {
                    return;
                }
                if s.hang_on.contains(&method) {
                    None
                } else if let Some(raw) = s.raw_override.get(&method) {
                    Some(raw.clone())
                } else {
                    let body = if s.error_on.contains(&method) {
                        json!({"jsonrpc": "2.0", "id": req["id"], "error": {"code": 1, "message": "boom"}})
                    } else {
                        let result = match s.result_override.get(&method) {
                            Some(v) => v.clone(),
                            None => default_result(&s, &method, &req["params"]),
                        };
                        json!({"jsonrpc": "2.0", "id": req["id"], "result": result, "error": null})
                    };
                    let mut out = Vec::new();
                    if s.noisy {
                        out.extend_from_slice(
                            b"{\"jsonrpc\":\"2.0\",\"method\":\"blockchain.headers.subscribe\",\"params\":[{}]}\n",
                        );
                        out.extend_from_slice(b"{\"jsonrpc\":\"2.0\",\"id\":999999,\"result\":null}\n");
                    }
                    out.extend_from_slice(body.to_string().as_bytes());
                    out.push(b'\n');
                    Some(out)
                }
            };
            match reply {
                Some(bytes) => w.write_all(&bytes).await.unwrap(),
                None => std::future::pending::<()>().await,
            }
        }
    }

    fn default_result(s: &State, method: &str, params: &Value) -> Value {
        match method {
            "server.version" => json!(["FakeElectrum 1.0", "1.4"]),
            "server.features" => json!({"genesis_hash": s.genesis}),
            "blockchain.headers.subscribe" => json!({"height": s.tip, "hex": "00"}),
            "blockchain.scripthash.get_history" => {
                let sh = params[0].as_str().unwrap();
                let entries: Vec<Value> = s
                    .history
                    .get(sh)
                    .map(|h| h.iter().map(|(t, hgt)| json!({"tx_hash": t, "height": hgt})).collect())
                    .unwrap_or_default();
                Value::Array(entries)
            }
            _ => {
                assert_eq!(method, "blockchain.transaction.get", "fake electrum: unexpected method");
                json!(s.txs[params[0].as_str().unwrap()])
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::testutil::*;
    use super::*;

    fn script(n: u8) -> ScriptBuf {
        let mut bytes = vec![0x00, 0x14];
        bytes.extend([n; 20]);
        ScriptBuf::from_bytes(bytes)
    }

    fn paid(value_sat: u64, height: Option<u32>) -> Payment {
        Payment { value_sat, height }
    }

    /// Error of a snapshot that fails as a whole.
    async fn err(f: &FakeElectrum, scripts: &[ScriptBuf]) -> String {
        f.client().snapshot(scripts).await.unwrap_err().to_string()
    }

    /// Error recorded for the first script of an otherwise successful snapshot.
    async fn script_err(f: &FakeElectrum, scripts: &[ScriptBuf]) -> String {
        f.client()
            .snapshot(scripts)
            .await
            .unwrap()
            .payments
            .remove(0)
            .unwrap_err()
    }

    #[tokio::test]
    async fn genesis_hash_and_missing_field() {
        let f = FakeElectrum::start().await;
        let c = f.client();
        assert_eq!(c.genesis_hash().await.unwrap(), REGTEST_GENESIS);
        f.set(|s| s.result_override.insert("server.features".into(), json!({})));
        assert!(c
            .genesis_hash()
            .await
            .unwrap_err()
            .to_string()
            .contains("lacks genesis_hash"));
        // One connection, one handshake, reused across calls.
        assert_eq!(f.set(|s| s.connections), 1);
        assert_eq!(f.set(|s| s.calls.iter().filter(|m| *m == "server.version").count()), 1);
    }

    #[tokio::test]
    async fn pinned_genesis_is_checked_on_every_connection() {
        let f = FakeElectrum::start().await;
        let a = script(1);
        let client = f.client().expect_genesis(REGTEST_GENESIS.to_uppercase());
        assert_eq!(client.snapshot(std::slice::from_ref(&a)).await.unwrap().tip, 100);

        // The backend is swapped for one on another chain; the next reconnect notices.
        f.set(|s| s.genesis = "00".repeat(32));
        f.set(|s| s.close_on.insert("blockchain.headers.subscribe".into()));
        assert!(client.snapshot(std::slice::from_ref(&a)).await.is_err());
        f.set(|s| s.close_on.clear());
        let e = client.snapshot(std::slice::from_ref(&a)).await.unwrap_err().to_string();
        assert!(e.contains("different chain"), "{e}");

        f.set(|s| s.result_override.insert("server.features".into(), json!({})));
        assert!(client
            .snapshot(std::slice::from_ref(&a))
            .await
            .unwrap_err()
            .to_string()
            .contains("different chain"));
        f.set(|s| s.close_on.insert("server.features".into()));
        assert!(client
            .snapshot(std::slice::from_ref(&a))
            .await
            .unwrap_err()
            .to_string()
            .contains("connection closed"));
    }

    #[tokio::test]
    async fn snapshot_reports_payments_and_caches_transactions() {
        let f = FakeElectrum::start().await;
        let (a, b, c) = (script(1), script(2), script(3));
        let t1 = f.pay(&a, 5_000, 90);
        f.pay(&a, 2_500, 0);
        f.pay(&b, 1_000, -1);
        f.set(|s| s.noisy = true);
        let client = f.client();
        let snap = client.snapshot(&[a.clone(), b.clone(), c.clone()]).await.unwrap();
        assert_eq!(snap.tip, 100);
        assert_eq!(
            snap.payments,
            vec![
                Ok(vec![paid(5_000, Some(90)), paid(2_500, None)]),
                Ok(vec![paid(1_000, None)]),
                Ok(vec![])
            ]
        );
        let gets = |f: &FakeElectrum| f.set(|s| s.calls.iter().filter(|m| *m == "blockchain.transaction.get").count());
        assert_eq!(gets(&f), 3);

        // Second poll: heights are re-read, transactions come from the cache.
        f.set_height(&a, &t1, 95);
        f.set(|s| s.tip = 101);
        let snap = client.snapshot(std::slice::from_ref(&a)).await.unwrap();
        assert_eq!(snap.tip, 101);
        assert_eq!(snap.payments[0].as_ref().unwrap()[0], paid(5_000, Some(95)));
        assert_eq!(gets(&f), 3);

        // No scripts at all is fine too.
        assert_eq!(client.snapshot(&[]).await.unwrap().payments.len(), 0);
    }

    #[tokio::test]
    async fn same_transaction_in_two_histories_is_fetched_once() {
        let f = FakeElectrum::start().await;
        let (a, b) = (script(1), script(2));
        let txid = f.pay(&a, 4_000, 50);
        // The same transaction also shows up in b's history but pays b nothing.
        f.set(|s| s.history.entry(scripthash(&b)).or_default().push((txid.clone(), 50)));
        let snap = f.client().snapshot(&[a, b]).await.unwrap();
        assert_eq!(snap.payments, vec![Ok(vec![paid(4_000, Some(50))]), Ok(vec![])]);
        assert_eq!(
            f.set(|s| s.calls.iter().filter(|m| *m == "blockchain.transaction.get").count()),
            1
        );
    }

    #[tokio::test]
    async fn a_transaction_listed_twice_is_counted_once() {
        let f = FakeElectrum::start().await;
        let a = script(1);
        let txid = f.pay(&a, 4_000, 50);
        f.set(|s| {
            s.history
                .get_mut(&scripthash(&a))
                .unwrap()
                .push((txid.to_uppercase(), 50))
        });
        let snap = f.client().snapshot(&[a]).await.unwrap();
        assert_eq!(snap.payments, vec![Ok(vec![paid(4_000, Some(50))])]);
    }

    #[tokio::test]
    async fn cache_is_reset_when_it_grows_too_large() {
        let f = FakeElectrum::start().await;
        let a = script(1);
        f.pay(&a, 1_234, 10);
        let client = f.client();
        {
            let mut cache = client.cache.lock().await;
            for i in 0..=CACHE_LIMIT {
                cache.insert((format!("{i:064x}"), ScriptBuf::new()), 1);
            }
        }
        let snap = client.snapshot(&[a]).await.unwrap();
        assert_eq!(snap.payments[0].as_ref().unwrap()[0].value_sat, 1_234);
        assert_eq!(client.cache.lock().await.len(), 1);
    }

    #[tokio::test]
    async fn a_malformed_reply_fails_the_whole_snapshot() {
        let a = script(1);
        let cases: Vec<(&str, Value, &str)> = vec![
            (
                "blockchain.headers.subscribe",
                json!({"height": "tall"}),
                "bad tip height",
            ),
            (
                "blockchain.headers.subscribe",
                json!({"height": 99_999_999_999u64}),
                "bad tip height",
            ),
            ("blockchain.scripthash.get_history", json!("nope"), "bad history"),
            (
                "blockchain.scripthash.get_history",
                json!([{"tx_hash": "zz", "height": 1}]),
                "bad tx_hash",
            ),
            (
                "blockchain.scripthash.get_history",
                json!([{"tx_hash": 5, "height": 1}]),
                "bad tx_hash",
            ),
            (
                "blockchain.scripthash.get_history",
                json!([{"tx_hash": "a".repeat(64), "height": "x"}]),
                "bad tx height",
            ),
        ];
        for (method, result, want) in cases {
            let f = FakeElectrum::start().await;
            f.pay(&a, 1_000, 5);
            f.set(|s| s.result_override.insert(method.into(), result));
            let e = err(&f, std::slice::from_ref(&a)).await;
            assert!(e.contains(want), "{method}: {e}");
        }
    }

    #[tokio::test]
    async fn a_problem_with_one_script_does_not_hide_the_others() {
        let (poisoned, healthy) = (script(1), script(2));

        // Someone sends hundreds of dust transactions to one invoice address.
        let f = FakeElectrum::start().await;
        f.pay(&healthy, 7_000, 60);
        f.set(|s| {
            let spam = (0..=MAX_HISTORY).map(|i| (format!("{i:064x}"), 10)).collect();
            s.history.insert(scripthash(&poisoned), spam);
        });
        let snap = f.client().snapshot(&[poisoned.clone(), healthy.clone()]).await.unwrap();
        assert_eq!(snap.payments[0], Err(format!("more than {MAX_HISTORY} transactions")));
        assert_eq!(snap.payments[1], Ok(vec![paid(7_000, Some(60))]));

        // The server cannot produce a transaction of the poisoned script.
        let f = FakeElectrum::start().await;
        f.pay(&healthy, 7_000, 60);
        f.set(|s| s.history.insert(scripthash(&poisoned), vec![("ab".repeat(32), 10)]));
        f.set(|s| s.close_on.insert("blockchain.transaction.get".into()));
        let client = f.client();
        let snap = client.snapshot(&[poisoned.clone(), healthy.clone()]).await.unwrap();
        assert!(snap.payments[0].as_ref().unwrap_err().contains("connection closed"));
        // The healthy script's transaction could not be fetched either while the
        // server kept hanging up, but once it behaves the healthy one recovers.
        assert!(snap.payments[1].is_err());
        f.set(|s| {
            s.close_on.clear();
            s.error_on.insert("blockchain.transaction.get".into());
        });
        assert!(client.snapshot(std::slice::from_ref(&poisoned)).await.unwrap().payments[0].is_err());
        f.set(|s| s.error_on.clear());
        f.set(|s| s.history.remove(&scripthash(&poisoned)));
        let snap = client.snapshot(&[poisoned, healthy]).await.unwrap();
        assert_eq!(snap.payments, vec![Ok(vec![]), Ok(vec![paid(7_000, Some(60))])]);
    }

    #[tokio::test]
    async fn bad_transactions_are_rejected_per_script() {
        let a = script(1);
        for (result, want) in [
            (json!(42), "bad transaction hex"),
            (json!("zz"), "bad transaction hex"),
            (json!("00ff"), "undecodable transaction"),
        ] {
            let f = FakeElectrum::start().await;
            f.pay(&a, 1_000, 5);
            f.set(|s| s.result_override.insert("blockchain.transaction.get".into(), result));
            assert_eq!(script_err(&f, std::slice::from_ref(&a)).await, want);
        }
    }

    #[tokio::test]
    async fn rejects_transaction_that_does_not_match_its_txid() {
        let f = FakeElectrum::start().await;
        let (a, b) = (script(1), script(2));
        let real = f.pay(&a, 1_000, 5);
        let other = f.pay(&b, 9_999_999, 5);
        // A lying server answers the request for `real` with a different, richer transaction.
        f.set(|s| {
            let swapped = s.txs[&other].clone();
            s.txs.insert(real.clone(), swapped);
        });
        assert_eq!(script_err(&f, &[a]).await, "transaction does not match its txid");
    }

    #[tokio::test]
    async fn transport_failures_and_reconnect() {
        let a = script(1);

        // JSON-RPC error.
        let f = FakeElectrum::start().await;
        f.set(|s| s.error_on.insert("blockchain.headers.subscribe".into()));
        assert!(err(&f, std::slice::from_ref(&a)).await.contains("server error"));

        // Not JSON.
        let f = FakeElectrum::start().await;
        f.set(|s| {
            s.raw_override
                .insert("blockchain.headers.subscribe".into(), b"<html>\n".to_vec())
        });
        assert!(err(&f, std::slice::from_ref(&a)).await.contains("malformed response"));

        // Connection dropped mid-exchange.
        let f = FakeElectrum::start().await;
        f.set(|s| s.close_on.insert("blockchain.headers.subscribe".into()));
        assert!(err(&f, std::slice::from_ref(&a)).await.contains("connection closed"));

        // Handshake refused.
        let f = FakeElectrum::start().await;
        f.set(|s| s.close_on.insert("server.version".into()));
        assert!(err(&f, std::slice::from_ref(&a)).await.contains("connection closed"));

        // Server goes silent: bounded by the timeout, then a fresh connection works.
        let f = FakeElectrum::start().await;
        f.set(|s| s.hang_on.insert("blockchain.headers.subscribe".into()));
        let client = f.client();
        assert!(client
            .snapshot(std::slice::from_ref(&a))
            .await
            .unwrap_err()
            .to_string()
            .contains("timed out"));
        f.set(|s| s.hang_on.clear());
        assert_eq!(client.snapshot(std::slice::from_ref(&a)).await.unwrap().tip, 100);
        assert_eq!(f.set(|s| s.connections), 2);

        // Nothing listening.
        let dead = Electrum::new("127.0.0.1:1", Duration::from_millis(500));
        assert!(dead.genesis_hash().await.is_err());
    }

    #[tokio::test]
    async fn oversized_or_truncated_lines_are_rejected() {
        let a = script(1);
        // Line ends without a newline because the peer closed.
        let f = FakeElectrum::start().await;
        f.set(|s| {
            s.raw_override
                .insert("blockchain.headers.subscribe".into(), b"{\"id\":1".to_vec());
            s.close_on.insert("blockchain.scripthash.get_history".into());
        });
        assert!(err(&f, std::slice::from_ref(&a))
            .await
            .contains("truncated or too large"));

        // A line longer than the cap is cut off rather than buffered forever.
        let f = FakeElectrum::start().await;
        let huge = vec![b' '; MAX_LINE_BYTES as usize + 16];
        f.set(|s| s.raw_override.insert("blockchain.headers.subscribe".into(), huge));
        let big = Electrum::new(f.addr.clone(), Duration::from_secs(20));
        assert!(big
            .snapshot(&[a])
            .await
            .unwrap_err()
            .to_string()
            .contains("truncated or too large"));
    }

    #[tokio::test]
    async fn output_values_cannot_overflow() {
        use bitcoin::consensus::encode::serialize_hex;
        use bitcoin::{absolute, transaction, Amount, TxOut};
        let f = FakeElectrum::start().await;
        let a = script(1);
        // A fabricated transaction whose outputs to the script sum past u64::MAX.
        let out = |v: u64| TxOut {
            value: Amount::from_sat(v),
            script_pubkey: a.clone(),
        };
        let tx = Transaction {
            version: transaction::Version::TWO,
            lock_time: absolute::LockTime::ZERO,
            input: vec![],
            output: vec![out(u64::MAX), out(5)],
        };
        let txid = tx.compute_txid().to_string();
        f.set(|s| {
            s.txs.insert(txid.clone(), serialize_hex(&tx));
            s.history.insert(scripthash(&a), vec![(txid.clone(), 10)]);
        });
        let snap = f.client().snapshot(&[a]).await.unwrap();
        assert_eq!(snap.payments, vec![Ok(vec![paid(u64::MAX, Some(10))])]);
    }

    #[tokio::test]
    async fn tls_with_a_pinned_certificate() {
        let (fake, fp) = FakeElectrum::start_tls().await;
        let pin = parse_fingerprint(&fp).unwrap();
        let client = Electrum::new(fake.addr.clone(), Duration::from_secs(5))
            .with_security(Security::Pinned(vec![[9; 32], pin]))
            .expect_genesis(testutil::REGTEST_GENESIS);
        assert_eq!(client.genesis_hash().await.unwrap(), testutil::REGTEST_GENESIS);
        assert_eq!(client.snapshot(&[]).await.unwrap().tip, 100);

        // Older servers that only speak TLS 1.2 work the same way.
        let (old, old_fp) = FakeElectrum::start_tls12().await;
        let pinned12 = Electrum::new(old.addr.clone(), Duration::from_secs(5))
            .with_security(Security::Pinned(vec![parse_fingerprint(&old_fp).unwrap()]));
        assert_eq!(pinned12.snapshot(&[]).await.unwrap().tip, 100);

        // Another certificate, or no pin at all on a self-signed server, is refused.
        let wrong =
            Electrum::new(fake.addr.clone(), Duration::from_secs(5)).with_security(Security::Pinned(vec![[9; 32]]));
        let e = wrong.genesis_hash().await.unwrap_err().to_string();
        assert!(
            e.contains("does not match the pinned fingerprint") && e.contains(&fp),
            "{e}"
        );
        let public = Electrum::new(fake.addr.clone(), Duration::from_secs(5)).with_security(Security::Tls);
        assert!(public.genesis_hash().await.unwrap_err().to_string().contains("TLS"));
        // Plain TCP to a TLS server gets nowhere.
        let plain = Electrum::new(fake.addr.clone(), Duration::from_millis(500));
        assert!(plain.genesis_hash().await.is_err());
    }

    #[tokio::test]
    async fn tls_to_a_plain_server_and_bad_names_fail() {
        let fake = FakeElectrum::start().await;
        let tls =
            Electrum::new(fake.addr.clone(), Duration::from_secs(2)).with_security(Security::Pinned(vec![[0; 32]]));
        assert!(tls.genesis_hash().await.unwrap_err().to_string().contains("TLS"));
        let port = fake.addr.rsplit_once(':').unwrap().1;
        let bad_name =
            Electrum::new(format!("[::ffff:127.0.0.1]x:{port}"), Duration::from_secs(2)).with_security(Security::Tls);
        assert!(bad_name.genesis_hash().await.is_err());
    }

    #[tokio::test]
    async fn fingerprints_can_be_read_and_parsed() {
        let (fake, fp) = FakeElectrum::start_tls().await;
        assert_eq!(fetch_fingerprint(&fake.addr, Duration::from_secs(5)).await.unwrap(), fp);
        let colons: String = fp
            .as_bytes()
            .chunks(2)
            .map(|c| std::str::from_utf8(c).unwrap().to_uppercase())
            .collect::<Vec<_>>()
            .join(":");
        assert_eq!(parse_fingerprint(&colons).map(hex::encode), Some(fp.clone()));
        assert_eq!(parse_fingerprint("abcd"), None);
        assert_eq!(parse_fingerprint(&"zz".repeat(32)), None);
        assert_eq!(fingerprint_hex(b"x").len(), 64);
        assert_eq!(
            (
                Security::Plain.label(),
                Security::Tls.label(),
                Security::Pinned(vec![]).label()
            ),
            ("tcp", "tls", "tls-pinned")
        );

        // Not TLS, nothing listening, or never answering.
        let plain = FakeElectrum::start().await;
        assert!(fetch_fingerprint(&plain.addr, Duration::from_secs(2)).await.is_err());
        let dead = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .unwrap()
            .local_addr()
            .unwrap()
            .to_string();
        assert!(fetch_fingerprint(&dead, Duration::from_secs(2)).await.is_err());
        let silent = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let e = fetch_fingerprint(&silent.local_addr().unwrap().to_string(), Duration::from_millis(200)).await;
        assert!(e.unwrap_err().to_string().contains("timed out"));
    }
}
