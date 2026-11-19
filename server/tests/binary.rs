//! Runs the real `cvmpay` binary: once with a broken configuration, once all
//! the way up against an in-process relay and Electrum stub, then stops it
//! with SIGTERM.

use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use nostr_relay_builder::MockRelay;

const BIN: &str = env!("CARGO_BIN_EXE_cvmpay");
const REGTEST_GENESIS: &str = "0f9188f13cb7b2c71f2a335e3a4fc328bf5beb436012afca590b1a11466e2206";
const TPUB: &str =
    "tpubD6NzVbkrYhZ4XgiXtGrdW5XDAPFCL9h7we1vwNCpn8tGbBcgfVYjXyhWo4E1xkh56hjod1RhGjxbaTLV3X4FyWuejifB9jusQ46QzG87VKp";

fn clean_command() -> Command {
    let mut cmd = Command::new(BIN);
    for (k, _) in std::env::vars() {
        if k.starts_with("CVMPAY_") {
            cmd.env_remove(k);
        }
    }
    cmd
}

/// Electrum stub that answers just enough for startup and idle polling.
fn electrum_stub() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap().to_string();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let stream = stream.unwrap();
            std::thread::spawn(move || {
                let mut out = stream.try_clone().unwrap();
                for line in BufReader::new(stream).lines() {
                    let Ok(line) = line else { break };
                    let id: u64 = line
                        .split("\"id\":")
                        .nth(1)
                        .unwrap()
                        .split([',', '}'])
                        .next()
                        .unwrap()
                        .parse()
                        .unwrap();
                    let result = if line.contains("server.features") {
                        format!("{{\"genesis_hash\":\"{REGTEST_GENESIS}\"}}")
                    } else if line.contains("headers.subscribe") {
                        "{\"height\":1,\"hex\":\"00\"}".to_string()
                    } else {
                        "[\"stub\",\"1.4\"]".to_string()
                    };
                    writeln!(out, "{{\"jsonrpc\":\"2.0\",\"id\":{id},\"result\":{result}}}").unwrap();
                }
            });
        }
    });
    addr
}

#[test]
fn exits_with_an_error_when_unconfigured() {
    let out = clean_command().output().unwrap();
    assert_eq!(out.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("cvmpay: invalid configuration: CVMPAY_RELAYS is required"),
        "{stderr}"
    );
}

#[test]
fn prints_the_public_key_it_will_serve_under() {
    let dir = tempfile::tempdir().unwrap();
    let data = dir.path().join("data");
    let run = || {
        clean_command()
            .arg("pubkey")
            .env("CVMPAY_DATA_DIR", &data)
            .output()
            .unwrap()
    };
    let first = run();
    assert_eq!(first.status.code(), Some(0));
    let key = String::from_utf8(first.stdout).unwrap();
    assert!(
        key.trim().len() == 64 && key.trim().bytes().all(|b| b.is_ascii_hexdigit()),
        "{key}"
    );
    assert_eq!(run().stdout, key.as_bytes(), "the identity is kept");
    assert!(data.join("server.nsec").exists());
}

#[test]
fn runs_the_offline_wallet_service() {
    let dir = tempfile::tempdir().unwrap();
    let public = dir.path().join("public");
    std::fs::create_dir(&public).unwrap();
    let wallet = |sub: &str| {
        let mut cmd = clean_command();
        cmd.args(["wallet", sub])
            .env("CVMPAY_DATA_DIR", dir.path().join("wallet"))
            .env("CVMPAY_XPUB_FILE", public.join("account.xpub"))
            .env("CVMPAY_NETWORK", "regtest");
        cmd
    };
    let mut child = wallet("serve").stdout(Stdio::piped()).spawn().unwrap();
    let deadline = Instant::now() + Duration::from_secs(20);
    while !public.join("ready").exists() {
        assert!(Instant::now() < deadline, "the wallet service did not start");
        std::thread::sleep(Duration::from_millis(50));
    }
    assert!(std::fs::read_to_string(public.join("account.xpub"))
        .unwrap()
        .starts_with("vpub"));
    let out = wallet("export").output().unwrap();
    assert_eq!(out.status.code(), Some(0));
    assert!(String::from_utf8_lossy(&out.stdout).contains("24. "));
    assert!(public.join("recovery-exported").exists());
    // It stays up until told to stop.
    assert!(child.try_wait().unwrap().is_none());
    assert!(Command::new("kill")
        .args(["-TERM", &child.id().to_string()])
        .status()
        .unwrap()
        .success());
    assert_eq!(child.wait().unwrap().code(), Some(0));
}

/// Start the binary against an in-process relay and Electrum stub, wait for it
/// to serve, then deliver `signal` and expect a clean exit.
async fn serves_until(signal: &str) {
    let relay = MockRelay::run().await.unwrap();
    let dir = tempfile::tempdir().unwrap();
    let mut child = clean_command()
        .env("CVMPAY_RELAYS", relay.url().await.to_string())
        .env("CVMPAY_ELECTRUM", electrum_stub())
        .env("CVMPAY_NETWORK", "regtest")
        .env("CVMPAY_XPUB", TPUB)
        .env("CVMPAY_PUBLIC_INVOICING", "true")
        .env("CVMPAY_DATA_DIR", dir.path())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();

    let pubkey_file = dir.path().join("server.pubkey");
    let deadline = Instant::now() + Duration::from_secs(30);
    while !pubkey_file.exists() {
        assert!(Instant::now() < deadline, "server did not come up");
        assert!(child.try_wait().unwrap().is_none(), "server exited early");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert_eq!(std::fs::read_to_string(&pubkey_file).unwrap().len(), 64);

    // Let the signal handlers get installed, then ask it to stop.
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert!(Command::new("kill")
        .arg(signal)
        .arg(child.id().to_string())
        .status()
        .unwrap()
        .success());
    let out = tokio::task::spawn_blocking(move || child.wait_with_output())
        .await
        .unwrap()
        .unwrap();
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(0), "{stderr}");
    assert!(stderr.contains("cvmpay is serving"), "{stderr}");
    assert!(stderr.contains("first receive address"), "{stderr}");
}

#[tokio::test(flavor = "multi_thread")]
async fn starts_serves_and_stops_on_sigterm() {
    serves_until("-TERM").await;
}

#[tokio::test(flavor = "multi_thread")]
async fn stops_on_ctrl_c() {
    serves_until("-INT").await;
}
