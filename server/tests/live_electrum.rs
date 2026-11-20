//! Read-only smoke test against a real Electrum server (Fulcrum, electrs, ...).
//!
//! Ignored by default. The address is taken from the environment only:
//!
//!   DEADHOP_TEST_ELECTRUM_HOST=<host> [DEADHOP_TEST_ELECTRUM_PORT=50001] \
//!     [DEADHOP_TEST_ELECTRUM_CERT_SHA256=<fingerprint>] \
//!     cargo test --test live_electrum -- --ignored --nocapture
//!
//! If the port speaks TLS, the certificate is pinned: to the given fingerprint,
//! or else to the one the server presents now (trust on first use, printed so
//! it can be compared with the server). It asks for the genesis hash and the
//! chain tip and nothing else.

use std::time::Duration;

use bitcoin::blockdata::constants::genesis_block;
use bitcoin::Network;
use deadhop::electrum::{fetch_fingerprint, parse_fingerprint, Chain, Electrum, Security};

#[tokio::test]
#[ignore = "needs a real Electrum server; set DEADHOP_TEST_ELECTRUM_HOST"]
async fn reads_genesis_and_tip() {
    // Values from an env file may carry stray spaces, quotes or a Windows line ending.
    let clean = |v: String| v.trim().trim_matches(|c| c == '"' || c == '\'').trim().to_string();
    let host = clean(std::env::var("DEADHOP_TEST_ELECTRUM_HOST").expect("set DEADHOP_TEST_ELECTRUM_HOST"));
    let port = clean(std::env::var("DEADHOP_TEST_ELECTRUM_PORT").unwrap_or_else(|_| "50001".into()));
    let addr = format!("{host}:{port}");
    let timeout = Duration::from_secs(15);

    let security = match fetch_fingerprint(&addr, timeout).await {
        Ok(seen) => {
            let pin = std::env::var("DEADHOP_TEST_ELECTRUM_CERT_SHA256")
                .map(clean)
                .unwrap_or_else(|_| seen.clone());
            println!("TLS certificate SHA-256: {seen}");
            assert_eq!(
                parse_fingerprint(&pin).map(hex::encode),
                Some(seen),
                "certificate differs from the pin"
            );
            // A different pin must be refused.
            let wrong = Electrum::new(addr.clone(), timeout).with_security(Security::Pinned(vec![[0; 32]]));
            assert!(wrong.genesis_hash().await.is_err(), "a wrong pin was accepted");
            Security::Pinned(vec![parse_fingerprint(&pin).unwrap()])
        }
        Err(e) => {
            println!("no TLS on this port ({e}); using plain TCP");
            Security::Plain
        }
    };
    let label = security.label();
    let client = Electrum::new(addr, timeout).with_security(security);
    let genesis = client.genesis_hash().await.expect("server.features");
    let network = [Network::Bitcoin, Network::Testnet, Network::Signet, Network::Regtest]
        .into_iter()
        .find(|n| genesis_block(*n).block_hash().to_string() == genesis);
    let tip = client.snapshot(&[]).await.expect("blockchain.headers.subscribe").tip;

    println!("electrum server over {label} is on {network:?} (genesis {genesis}), tip height {tip}");
    assert!(network.is_some(), "unknown chain");
    assert!(tip > 0);
}
