//! Per-relay health, beyond "is the socket open": can the relay be reached,
//! does it carry an ephemeral ContextVM event from one connection to another,
//! and is it rate-limiting us.
//!
//! Each check runs on its own short-lived connections, under a throwaway key
//! and through the configured proxy, so it never disturbs the connections the
//! service answers on and tells the relay nothing about the service.

use std::time::{Duration, Instant};

use nostr_sdk::prelude::{RelayMessage, RelayPoolNotification};
use nostr_sdk::{Client, EventBuilder, Filter, Keys, Kind, Tag, Timestamp};
use rmcp::schemars::{self, JsonSchema};
use serde::Serialize;

use crate::relay::{ProxySpec, ProxyTransport};

/// The event kind ContextVM requests and replies use (ephemeral).
pub const CTXVM_KIND: u16 = 25910;

/// One relay's health.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct RelayHealth {
    pub relay: String,
    /// `ok` (every check passed), `degraded` (connects, but something failed)
    /// or `down` (cannot connect).
    pub status: String,
    pub connects: bool,
    /// Whether a ContextVM event sent through the relay came back to a second
    /// connection; `None` when it could not be tried.
    pub forwards: Option<bool>,
    pub rate_limited: bool,
    /// Round trip of the test event, in milliseconds.
    pub ms: Option<u64>,
    /// What went wrong, in one sentence; empty when nothing did.
    pub detail: String,
    pub checked_at: u64,
}

/// Whether a relay's message says it is limiting us.
fn is_rate_limit(text: &str) -> bool {
    let t = text.to_ascii_lowercase();
    ["rate-limit", "rate limit", "too many", "slow down"]
        .iter()
        .any(|w| t.contains(w))
}

fn client(proxy: Option<&ProxySpec>) -> Client {
    let builder = Client::builder().signer(Keys::generate());
    match proxy {
        Some(spec) => builder.websocket_transport(ProxyTransport::new(spec.clone())).build(),
        None => builder.build(),
    }
}

async fn connected(c: &Client) -> bool {
    c.relays().await.values().any(|r| r.is_connected())
}

/// Check one relay, giving each step at most `limit`.
pub async fn check_relay(url: &str, proxy: Option<&ProxySpec>, limit: Duration, now: u64) -> RelayHealth {
    let mut h = RelayHealth {
        relay: url.to_string(),
        status: "down".into(),
        connects: false,
        forwards: None,
        rate_limited: false,
        ms: None,
        detail: String::new(),
        checked_at: now,
    };
    let (listener, sender) = (client(proxy), client(proxy));
    if listener.add_relay(url).await.is_err() || sender.add_relay(url).await.is_err() {
        h.detail = "Not a relay address.".into();
        return h;
    }
    listener.connect().await;
    sender.connect().await;
    listener.wait_for_connection(limit).await;
    sender.wait_for_connection(limit).await;
    if !(connected(&listener).await && connected(&sender).await) {
        h.detail = "Could not connect to the relay.".into();
        listener.disconnect().await;
        sender.disconnect().await;
        return h;
    }
    h.connects = true;
    probe(&listener, &sender, limit, &mut h).await;
    listener.disconnect().await;
    sender.disconnect().await;
    h.status = if h.forwards == Some(true) && !h.rate_limited {
        "ok"
    } else {
        "degraded"
    }
    .into();
    h
}

/// Send an ephemeral ContextVM event from `sender` and wait for it at `listener`.
async fn probe(listener: &Client, sender: &Client, limit: Duration, h: &mut RelayHealth) {
    let target = Keys::generate().public_key();
    let mut notes = listener.notifications();
    let since = Timestamp::from(Timestamp::now().as_secs().saturating_sub(10));
    let filter = Filter::new().kind(Kind::Custom(CTXVM_KIND)).pubkey(target).since(since);
    // A subscription that fails shows up below as an event that never arrives.
    let _ = listener.subscribe(filter, None).await;
    // Let the subscription reach the relay before the event does.
    tokio::time::sleep(limit / 10).await;
    let event = EventBuilder::new(Kind::Custom(CTXVM_KIND), "deadhop relay check")
        .tag(Tag::public_key(target))
        .sign_with_keys(&Keys::generate())
        .expect("a fresh key signs");
    let started = Instant::now();
    let sent = tokio::time::timeout(limit, sender.send_event(&event))
        .await
        .ok()
        .and_then(Result::ok);
    let refusal = match sent {
        Some(out) if !out.success.is_empty() => None,
        Some(out) => Some(out.failed.into_values().collect::<Vec<_>>().join("; ")),
        None => Some("no answer".into()),
    };
    if let Some(why) = refusal {
        h.forwards = Some(false);
        h.rate_limited = is_rate_limit(&why);
        h.detail = if h.rate_limited {
            format!("The relay is rate-limiting us: {why}.")
        } else {
            format!("The relay did not take the test event: {why}.")
        };
        return;
    }
    let wait = async {
        let mut limited = false;
        loop {
            // The listener holds the sending side, so the channel stays open.
            match notes.recv().await.ok() {
                Some(RelayPoolNotification::Event { event: got, .. }) if got.id == event.id => return (true, limited),
                Some(RelayPoolNotification::Message {
                    message: RelayMessage::Notice(n),
                    ..
                }) if is_rate_limit(&n) => limited = true,
                _ => {}
            }
        }
    };
    let (arrived, limited) = tokio::time::timeout(limit, wait).await.unwrap_or((false, false));
    h.forwards = Some(arrived);
    h.rate_limited = limited;
    if arrived {
        h.ms = Some(started.elapsed().as_millis() as u64);
    }
    h.detail = match (arrived, limited) {
        (true, false) => String::new(),
        (true, true) => "The relay carries ContextVM events, but warned that it is rate-limiting us.".into(),
        (false, _) => "The relay took the test event but did not pass it on to another connection.".into(),
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures_util::{SinkExt, StreamExt};
    use nostr_relay_builder::MockRelay;
    use tokio::net::TcpListener;
    use tokio::task::JoinHandle;
    use tokio_tungstenite::tungstenite::Message as Wire;

    const LIMIT: Duration = Duration::from_secs(2);

    /// How a fake relay answers.
    #[derive(Clone, Copy)]
    enum Fake {
        /// `OK false` with this message for every event.
        Refuse(&'static str),
        /// `OK true`, but the event is never passed on.
        Swallow,
        /// Passes events on, but sends a NOTICE about limits on every REQ.
        Notice,
        /// Never answers an event.
        Silent,
    }

    async fn fake_relay(kind: Fake) -> (String, JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("ws://{}", listener.local_addr().unwrap());
        let (tx, _) = tokio::sync::broadcast::channel::<String>(16);
        let task = tokio::spawn(async move {
            loop {
                let (tcp, _) = listener.accept().await.unwrap();
                let (tx, mut rx) = (tx.clone(), tx.subscribe());
                tokio::spawn(async move {
                    let mut ws = tokio_tungstenite::accept_async(tcp).await.unwrap();
                    let mut sub = None;
                    loop {
                        tokio::select! {
                            frame = ws.next() => {
                                let Some(Ok(frame)) = frame else { return };
                                let Wire::Text(text) = frame else { continue };
                                let msg: serde_json::Value = serde_json::from_str(&text).unwrap();
                                let reply = match (msg[0].as_str().unwrap(), kind) {
                                    ("REQ", Fake::Notice) => {
                                        sub = Some(msg[1].clone());
                                        serde_json::json!(["NOTICE", "rate limited: slow down"])
                                    }
                                    ("EVENT", Fake::Refuse(why)) => serde_json::json!(["OK", msg[1]["id"], false, why]),
                                    ("EVENT", Fake::Swallow) => serde_json::json!(["OK", msg[1]["id"], true, ""]),
                                    ("EVENT", Fake::Notice) => {
                                        let _ = tx.send(msg[1].to_string());
                                        serde_json::json!(["OK", msg[1]["id"], true, ""])
                                    }
                                    ("EVENT", Fake::Silent) => continue,
                                    // REQ, and anything else a client sends.
                                    _ => {
                                        sub = Some(msg[1].clone());
                                        serde_json::json!(["EOSE", msg[1]])
                                    }
                                };
                                ws.send(Wire::Text(reply.to_string().into())).await.unwrap();
                            }
                            Ok(event) = rx.recv() => {
                                if let Some(id) = &sub {
                                    let out = format!("[\"EVENT\",{id},{event}]");
                                    ws.send(Wire::Text(out.into())).await.unwrap();
                                }
                            }
                        }
                    }
                });
            }
        });
        (url, task)
    }

    #[test]
    fn rate_limits_are_recognised_in_relay_messages() {
        for m in [
            "rate-limited: slow down",
            "Rate limit exceeded",
            "too many requests",
            "please slow down",
        ] {
            assert!(is_rate_limit(m), "{m}");
        }
        assert!(!is_rate_limit("blocked: pubkey not allowed"));
    }

    #[tokio::test]
    async fn a_working_relay_is_ok_with_a_round_trip_time() {
        let relay = MockRelay::run().await.unwrap();
        let h = check_relay(&relay.url().await.to_string(), None, LIMIT, 7).await;
        assert_eq!(
            (h.status.as_str(), h.connects, h.forwards, h.rate_limited),
            ("ok", true, Some(true), false)
        );
        assert!(h.ms.is_some() && h.detail.is_empty() && h.checked_at == 7, "{h:?}");
    }

    #[tokio::test]
    async fn a_relay_that_refuses_or_limits_is_degraded() {
        let (url, _t) = fake_relay(Fake::Refuse("rate-limited: slow down")).await;
        let h = check_relay(&url, None, LIMIT, 1).await;
        assert_eq!(
            (h.status.as_str(), h.connects, h.forwards, h.rate_limited),
            ("degraded", true, Some(false), true)
        );
        assert!(h.detail.starts_with("The relay is rate-limiting us"), "{}", h.detail);

        let (url, _t) = fake_relay(Fake::Refuse("blocked: not allowed")).await;
        let h = check_relay(&url, None, LIMIT, 1).await;
        assert_eq!((h.status.as_str(), h.rate_limited), ("degraded", false));
        assert!(h.detail.contains("blocked: not allowed"), "{}", h.detail);

        let (url, _t) = fake_relay(Fake::Swallow).await;
        let h = check_relay(&url, None, LIMIT, 1).await;
        assert_eq!((h.status.as_str(), h.forwards), ("degraded", Some(false)));
        assert!(h.detail.contains("did not pass it on"), "{}", h.detail);

        let (url, _t) = fake_relay(Fake::Silent).await;
        let h = check_relay(&url, None, LIMIT, 1).await;
        assert_eq!((h.status.as_str(), h.forwards), ("degraded", Some(false)));
        assert!(h.detail.contains("no answer"), "{}", h.detail);

        let (url, _t) = fake_relay(Fake::Notice).await;
        let h = check_relay(&url, None, LIMIT, 1).await;
        assert_eq!(
            (h.status.as_str(), h.forwards, h.rate_limited),
            ("degraded", Some(true), true)
        );
        assert!(h.detail.contains("warned"), "{}", h.detail);
    }

    #[tokio::test]
    async fn a_relay_that_cannot_be_reached_is_down() {
        // Nothing listens there.
        let h = check_relay("ws://127.0.0.1:9", None, LIMIT, 1).await;
        assert_eq!((h.status.as_str(), h.connects, h.forwards), ("down", false, None));
        assert_eq!(h.detail, "Could not connect to the relay.");
        let h = check_relay("not a url", None, LIMIT, 1).await;
        assert_eq!((h.status.as_str(), h.detail.as_str()), ("down", "Not a relay address."));
    }

    #[tokio::test]
    async fn checks_go_through_the_proxy() {
        let relay = MockRelay::run().await.unwrap();
        let (port, _p) = crate::relay::tests::socks5_proxy(None).await;
        let spec = crate::relay::parse_proxy(&format!("socks5://127.0.0.1:{port}")).unwrap();
        let h = check_relay(&relay.url().await.to_string(), spec.as_ref(), LIMIT, 1).await;
        assert_eq!(h.status, "ok", "{h:?}");
    }
}
