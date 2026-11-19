//! The offline wallet service (`cvmpay wallet ...`).
//!
//! For operators who do not bring their own `CVMPAY_XPUB`: on first start it
//! generates a BIP-39 recovery phrase in its own data directory and hands only
//! the account's public key to the payment service, through the file named by
//! `CVMPAY_XPUB_FILE`. It is meant to run in a container without network
//! access, so the internet-facing payment service never holds a spending key.
//! `cvmpay wallet export` prints the phrase, so the funds can be spent from
//! any ordinary bitcoin wallet.

use std::future::Future;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::process::ExitCode;

use bip39::Mnemonic;
use bitcoin::Network;

use crate::app::{create_private, read_private};
use crate::config::parse_network;
use crate::error::{Error, Result};
use crate::wallet::{account_from_phrase, account_path, encode_account, Wallet};

/// Recovery phrase, inside the wallet service's data directory.
pub const PHRASE_FILE: &str = "wallet.mnemonic";
/// Next to the xpub file: present once the wallet service has started.
pub const READY_FILE: &str = "ready";
/// Next to the xpub file: present when the xpub was generated here.
pub const GENERATED_FILE: &str = "generated";
/// Next to the xpub file: when the recovery phrase was last exported.
pub const EXPORTED_FILE: &str = "recovery-exported";

/// What the wallet commands read from the environment.
struct Env {
    dir: PathBuf,
    network: Network,
    xpub_file: PathBuf,
    own_xpub: bool,
}

fn env(lookup: &dyn Fn(&str) -> Option<String>) -> Result<Env> {
    let get = |k: &str| lookup(k).map(|v| v.trim().to_string()).filter(|v| !v.is_empty());
    let dir = PathBuf::from(get("CVMPAY_DATA_DIR").unwrap_or("data".into()));
    Ok(Env {
        network: parse_network(&get("CVMPAY_NETWORK").unwrap_or("bitcoin".into()))?,
        xpub_file: get("CVMPAY_XPUB_FILE").map_or(dir.join("account.xpub"), PathBuf::from),
        own_xpub: get("CVMPAY_XPUB").is_some(),
        dir,
    })
}

/// A file next to the xpub file.
fn beside(xpub_file: &Path, name: &str) -> PathBuf {
    xpub_file.with_file_name(name)
}

fn parse_phrase(text: &str, path: &Path) -> Result<Mnemonic> {
    Mnemonic::parse(text.trim())
        .map_err(|_| Error::Config(format!("{} does not contain a BIP-39 recovery phrase", path.display())))
}

/// Load the recovery phrase, creating a 24-word one on first start.
pub fn load_or_create_phrase(path: &Path) -> Result<Mnemonic> {
    if let Some(text) = read_private(path)? {
        return parse_phrase(&text, path);
    }
    let mut entropy = [0u8; 32];
    getrandom::getrandom(&mut entropy).expect("the operating system provides randomness");
    let phrase = Mnemonic::from_entropy(&entropy).expect("32 bytes is a valid BIP-39 entropy length");
    create_private(path, &phrase.to_string())?;
    Ok(phrase)
}

/// Replace a public file atomically (readable by other users).
fn write_public(path: &Path, content: &str) -> Result<()> {
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, content)?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}

/// Set up the wallet and publish its public key. Returns what to tell the operator.
fn init(env: &Env) -> Result<String> {
    if env.own_xpub {
        write_public(&beside(&env.xpub_file, READY_FILE), "own xpub\n")?;
        return Ok("Using your own CVMPAY_XPUB: no wallet is generated here.\n".into());
    }
    crate::app::open_dir(&env.dir)?;
    let phrase_path = env.dir.join(PHRASE_FILE);
    let account = account_from_phrase(&load_or_create_phrase(&phrase_path)?, env.network);
    let key = encode_account(&account);
    let first = Wallet::new(&key, env.network)?.address(0)?;
    write_public(&env.xpub_file, &format!("{key}\n"))?;
    write_public(&beside(&env.xpub_file, GENERATED_FILE), "generated\n")?;
    write_public(&beside(&env.xpub_file, READY_FILE), "generated\n")?;
    let backup = if beside(&env.xpub_file, EXPORTED_FILE).exists() {
        "The recovery phrase has been exported.".to_string()
    } else {
        "Back up the recovery phrase now: cvmpay wallet export".to_string()
    };
    Ok(format!(
        "Wallet generated on this host. The payment service gets only its public key.\n\
         Account (BIP-84, native segwit, {path}): {key}\n\
         First address: {first}\n\
         {backup}\n",
        path = account_path(env.network),
    ))
}

/// The recovery phrase and how to restore it, for `cvmpay wallet export`.
fn export(env: &Env, now: u64) -> Result<String> {
    if env.own_xpub {
        return Err(Error::Config(
            "this service uses your own CVMPAY_XPUB, so its recovery phrase is in your wallet, not here".into(),
        ));
    }
    let path = env.dir.join(PHRASE_FILE);
    let text = read_private(&path)?.ok_or_else(|| {
        Error::Config(format!(
            "no wallet in {}: start the wallet service first",
            env.dir.display()
        ))
    })?;
    let phrase = parse_phrase(&text, &path)?;
    let words: Vec<String> = phrase
        .words()
        .enumerate()
        .map(|(i, w)| format!("{:>2}. {w:<10}", i + 1))
        .collect();
    let grid: Vec<String> = words
        .chunks(4)
        .map(|row| row.join(" ").trim_end().to_string())
        .collect();
    write_public(&beside(&env.xpub_file, EXPORTED_FILE), &format!("{now}\n"))?;
    Ok(format!(
        "Recovery phrase ({n} words). Anyone who has these words can spend the funds.\n\
         Write them down and keep them offline. Do not photograph or paste them.\n\n\
         {grid}\n\n\
         To spend, restore the words in any bitcoin wallet (for example Sparrow, BlueWallet or Nunchuk):\n\
         - wallet type: native segwit (BIP-84), derivation path {path}, no passphrase\n\
         - account key, to check it is the same wallet: {key}\n\
         - if the balance looks too low, raise the wallet's gap limit to the number of\n  \
         addresses the dashboard shows as created, plus 20\n",
        n = words.len(),
        grid = grid.join("\n"),
        path = account_path(env.network),
        key = encode_account(&account_from_phrase(&phrase, env.network)),
    ))
}

fn report(result: Result<String>, out: &mut dyn Write) -> ExitCode {
    match result {
        Ok(text) => {
            let _ = out.write_all(text.as_bytes());
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("cvmpay wallet: {e}");
            ExitCode::FAILURE
        }
    }
}

/// `cvmpay wallet export`.
pub fn export_command(lookup: &dyn Fn(&str) -> Option<String>, now: u64, out: &mut dyn Write) -> ExitCode {
    report(env(lookup).and_then(|e| export(&e, now)), out)
}

/// `cvmpay wallet serve`: set up the wallet, publish its key, then stay up
/// (for `export`) until `shutdown`.
pub async fn serve(
    lookup: &dyn Fn(&str) -> Option<String>,
    shutdown: Pin<Box<dyn Future<Output = ()> + '_>>,
    out: &mut dyn Write,
) -> ExitCode {
    let code = report(env(lookup).and_then(|e| init(&e)), out);
    if code == ExitCode::SUCCESS {
        shutdown.await;
    }
    code
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::os::unix::fs::PermissionsExt;

    fn lookup<'a>(m: &'a HashMap<&'static str, String>) -> impl Fn(&str) -> Option<String> + 'a {
        |k| m.get(k).cloned()
    }

    fn vars(dir: &Path) -> HashMap<&'static str, String> {
        HashMap::from([
            ("CVMPAY_DATA_DIR", dir.join("wallet").to_string_lossy().into_owned()),
            (
                "CVMPAY_XPUB_FILE",
                dir.join("public/account.xpub").to_string_lossy().into_owned(),
            ),
            ("CVMPAY_NETWORK", "regtest".to_string()),
        ])
    }

    async fn serve_once(m: &HashMap<&'static str, String>) -> (ExitCode, String) {
        let mut out = Vec::new();
        let code = serve(&lookup(m), Box::pin(std::future::ready(())), &mut out).await;
        (code, String::from_utf8(out).unwrap())
    }

    #[tokio::test]
    async fn generates_once_publishes_the_public_key_and_exports_the_phrase() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir(tmp.path().join("public")).unwrap();
        let m = vars(tmp.path());
        let (code, text) = serve_once(&m).await;
        assert!(code == ExitCode::SUCCESS, "{text}");
        assert!(text.contains("Back up the recovery phrase now"), "{text}");
        let key = std::fs::read_to_string(tmp.path().join("public/account.xpub")).unwrap();
        assert!(key.starts_with("vpub"), "{key}");
        assert!(tmp.path().join("public/generated").exists());
        assert!(tmp.path().join("public/ready").exists());
        let phrase = tmp.path().join("wallet").join(PHRASE_FILE);
        assert_eq!(std::fs::metadata(&phrase).unwrap().permissions().mode() & 0o777, 0o600);
        assert_eq!(std::fs::read_to_string(&phrase).unwrap().split(' ').count(), 24);

        // The payment service reads the published key.
        let wallet = Wallet::new(key.trim(), Network::Regtest).unwrap();
        assert!(text.contains(&wallet.address(0).unwrap().to_string()));

        // A restart keeps the same wallet.
        let (_, again) = serve_once(&m).await;
        assert_eq!(
            std::fs::read_to_string(tmp.path().join("public/account.xpub")).unwrap(),
            key
        );
        assert!(again.contains(key.trim()));

        let mut out = Vec::new();
        assert!(export_command(&lookup(&m), 42, &mut out) == ExitCode::SUCCESS);
        let exported = String::from_utf8(out).unwrap();
        let words = std::fs::read_to_string(&phrase).unwrap();
        for (i, w) in words.split(' ').enumerate() {
            assert!(
                exported.contains(&format!("{:>2}. {w}", i + 1)),
                "word {i} in {exported}"
            );
        }
        assert!(exported.contains("m/84'/1'/0'") && exported.contains(key.trim()));
        assert_eq!(
            std::fs::read_to_string(tmp.path().join("public/recovery-exported")).unwrap(),
            "42\n"
        );
        let (_, after) = serve_once(&m).await;
        assert!(after.contains("has been exported"), "{after}");
    }

    #[tokio::test]
    async fn an_own_xpub_means_no_wallet_here() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir(tmp.path().join("public")).unwrap();
        let mut m = vars(tmp.path());
        m.insert("CVMPAY_XPUB", "tpub...".into());
        let (code, text) = serve_once(&m).await;
        assert!(code == ExitCode::SUCCESS);
        assert!(text.contains("own CVMPAY_XPUB"));
        assert!(tmp.path().join("public/ready").exists());
        assert!(!tmp.path().join("public/account.xpub").exists());
        assert!(!tmp.path().join("wallet").exists());
        let mut out = Vec::new();
        assert!(export_command(&lookup(&m), 1, &mut out) == ExitCode::FAILURE);
        assert!(out.is_empty());
    }

    #[tokio::test]
    async fn problems_are_reported() {
        let tmp = tempfile::tempdir().unwrap();
        let mut m = vars(tmp.path());
        // Nothing to export before the wallet exists.
        let mut out = Vec::new();
        assert!(export_command(&lookup(&m), 1, &mut out) == ExitCode::FAILURE);
        // The xpub's directory does not exist.
        assert!(serve_once(&m).await.0 == ExitCode::FAILURE);
        m.insert("CVMPAY_NETWORK", "dogecoin".into());
        assert!(serve_once(&m).await.0 == ExitCode::FAILURE);
        // A damaged phrase file is refused, not replaced.
        m.insert("CVMPAY_NETWORK", "regtest".into());
        std::fs::create_dir(tmp.path().join("public")).unwrap();
        // The failed start above already made the phrase; damage it.
        let phrase = tmp.path().join("wallet").join(PHRASE_FILE);
        std::fs::remove_file(&phrase).unwrap();
        create_private(&phrase, "not words").unwrap();
        assert!(serve_once(&m).await.0 == ExitCode::FAILURE);
        assert!(export_command(&lookup(&m), 1, &mut Vec::new()) == ExitCode::FAILURE);
    }

    #[test]
    fn defaults_use_the_data_directory_and_mainnet() {
        let e = env(&|_| None).unwrap();
        assert_eq!(e.dir, PathBuf::from("data"));
        assert_eq!(e.xpub_file, PathBuf::from("data/account.xpub"));
        assert_eq!(e.network, Network::Bitcoin);
        assert!(!e.own_xpub);
    }
}
