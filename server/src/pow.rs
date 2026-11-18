//! Admission of state-changing requests: NIP-13 proof of work plus replay and
//! audience checks on the client's signed request event.
//!
//! Nostr identities are free, so the cost that slows down sybil and flooding
//! attacks has to be attached to each state-creating request. The client mines
//! the id of the very event that carries the request. The server checks that
//! the event was signed by the caller, is addressed to this service, is fresh,
//! was never used before and - unless the caller is an operator - carries the
//! required work. Operators skip only the work: without the other checks a
//! signed operator request captured anywhere could be replayed here forever.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, AtomicU8, Ordering};
use std::sync::Mutex;

use nostr_sdk::Event;

use crate::error::{Error, Result};

/// Number of leading zero bits in an event id.
pub fn leading_zero_bits(id: &[u8; 32]) -> u32 {
    let mut bits = 0;
    for b in id {
        if *b == 0 {
            bits += 8;
        } else {
            bits += b.leading_zeros();
            break;
        }
    }
    bits
}

/// Difficulty the miner committed to in the `nonce` tag, if any.
fn committed_target(event: &Event) -> Option<u32> {
    event.tags.iter().find_map(|t| {
        let t = t.as_slice();
        if t.first().map(String::as_str) == Some("nonce") {
            t.get(2).and_then(|d| d.parse::<u32>().ok())
        } else {
            None
        }
    })
}

/// Kind of a ContextVM request event.
pub const CTXVM_KIND: u16 = 25910;

/// Whether the event names `pubkey` (hex) as its recipient.
fn addressed_to(event: &Event, pubkey: &str) -> bool {
    event.tags.iter().any(|t| {
        let t = t.as_slice();
        t.first().map(String::as_str) == Some("p") && t.get(1).map(String::as_str) == Some(pubkey)
    })
}

/// Verifies request events and remembers them to stop replays.
pub struct PowGuard {
    bits: AtomicU8,
    window_secs: AtomicU64,
    capacity: usize,
    /// Hex public key of this service.
    audience: String,
    /// event id -> time after which the entry may be forgotten
    seen: Mutex<HashMap<[u8; 32], u64>>,
}

impl PowGuard {
    /// `bits` of work required, events accepted within `window_secs` of now
    /// and only if addressed to `audience` (this service's hex public key).
    pub fn new(bits: u8, window_secs: u64, capacity: usize, audience: String) -> Self {
        PowGuard {
            bits: AtomicU8::new(bits),
            window_secs: AtomicU64::new(window_secs),
            capacity,
            audience,
            seen: Mutex::new(HashMap::new()),
        }
    }

    /// Required difficulty.
    pub fn bits(&self) -> u8 {
        self.bits.load(Ordering::Relaxed)
    }

    /// Accepted age of a request event, in seconds.
    pub fn window_secs(&self) -> u64 {
        self.window_secs.load(Ordering::Relaxed)
    }

    /// Change the difficulty and window for requests from now on.
    pub fn set_policy(&self, bits: u8, window_secs: u64) {
        self.bits.store(bits, Ordering::Relaxed);
        self.window_secs.store(window_secs, Ordering::Relaxed);
    }

    /// Accept `event` as the one-time carrier of a request by `caller` (hex
    /// pubkey). `exempt` callers (operators) owe no work.
    pub fn check(&self, event: &Event, caller: &str, now: u64, exempt: bool) -> Result<()> {
        if event.pubkey.to_hex() != caller {
            return Err(Error::Pow("event is not signed by the caller".into()));
        }
        if event.kind.as_u16() != CTXVM_KIND || !addressed_to(event, &self.audience) {
            return Err(Error::Pow("event is not a request addressed to this service".into()));
        }
        let (bits, window_secs) = (self.bits(), self.window_secs());
        let created = event.created_at.as_secs();
        if created.abs_diff(now) > window_secs {
            return Err(Error::Pow("event timestamp outside the accepted window".into()));
        }
        let id = event.id.to_bytes();
        if bits > 0 && !exempt {
            let required = u32::from(bits);
            if leading_zero_bits(&id) < required {
                return Err(Error::Pow(format!("{required} bits of NIP-13 work required")));
            }
            // The committed target stops a miner aiming low from getting lucky.
            if committed_target(event).is_none_or(|t| t < required) {
                return Err(Error::Pow(format!("nonce tag must commit to at least {required} bits")));
            }
        }
        // Cheap checks first; signature verification is the expensive one.
        if event.verify().is_err() {
            return Err(Error::Pow("invalid event id or signature".into()));
        }

        let mut seen = self.seen.lock().unwrap_or_else(|p| p.into_inner());
        if seen.len() >= self.capacity {
            seen.retain(|_, forget_at| *forget_at > now);
            if seen.len() >= self.capacity {
                return Err(Error::Capacity);
            }
        }
        // Keep the id for as long as its timestamp could still be accepted.
        if seen.insert(id, created + 2 * window_secs).is_some() {
            return Err(Error::Pow("event already used".into()));
        }
        Ok(())
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use nostr_sdk::{EventBuilder, Keys, Kind, Tag, Timestamp};

    /// Public key the test service answers to.
    pub const SERVICE: &str = "c6047f9441ed7d6d3045406e95c07cd85c778e4b8cef3ca7abac09b95c709ee5";

    fn to_service() -> Tag {
        Tag::parse(["p", SERVICE]).unwrap()
    }

    fn guard(bits: u8, window: u64, capacity: usize) -> PowGuard {
        PowGuard::new(bits, window, capacity, SERVICE.to_string())
    }

    /// A request event addressed to the test service with `bits` of work.
    pub fn mined(keys: &Keys, bits: u8, created_at: u64) -> Event {
        EventBuilder::new(Kind::Custom(CTXVM_KIND), "{}")
            .tag(to_service())
            .custom_created_at(Timestamp::from(created_at))
            .pow(bits)
            .sign_with_keys(keys)
            .unwrap()
    }

    /// Sign events with the given extra tags until one passes `accept`.
    fn search(keys: &Keys, tags: Vec<Tag>, accept: impl Fn(&Event) -> bool) -> Event {
        (0..)
            .map(|i| {
                EventBuilder::new(Kind::Custom(CTXVM_KIND), format!("{i}"))
                    .tag(to_service())
                    .tags(tags.clone())
                    .custom_created_at(Timestamp::from(1_000))
                    .sign_with_keys(keys)
                    .unwrap()
            })
            .find(accept)
            .unwrap()
    }

    fn msg(r: Result<()>) -> String {
        r.unwrap_err().to_string()
    }

    #[test]
    fn counts_leading_zero_bits() {
        let mut id = [0u8; 32];
        assert_eq!(leading_zero_bits(&id), 256);
        id[0] = 0x80;
        assert_eq!(leading_zero_bits(&id), 0);
        id[0] = 0x01;
        assert_eq!(leading_zero_bits(&id), 7);
        id[0] = 0;
        id[2] = 0x10;
        assert_eq!(leading_zero_bits(&id), 19);
    }

    #[test]
    fn accepts_valid_work_once() {
        let keys = Keys::generate();
        let me = keys.public_key().to_hex();
        let guard = guard(8, 600, 100);
        assert_eq!(guard.bits(), 8);
        let ev = mined(&keys, 8, 1_000);
        guard.check(&ev, &me, 1_100, false).unwrap();
        assert!(msg(guard.check(&ev, &me, 1_100, false)).contains("already used"));
        // More work than required is fine.
        guard.check(&mined(&keys, 12, 1_001), &me, 1_100, false).unwrap();
    }

    #[test]
    fn rejects_wrong_signer_stale_and_weak_events() {
        let keys = Keys::generate();
        let me = keys.public_key().to_hex();
        let guard = guard(8, 600, 100);

        let other = Keys::generate().public_key().to_hex();
        assert!(msg(guard.check(&mined(&keys, 8, 1_000), &other, 1_000, false)).contains("not signed by the caller"));
        assert!(msg(guard.check(&mined(&keys, 8, 1_000), &me, 1_601, false)).contains("outside the accepted window"));
        assert!(msg(guard.check(&mined(&keys, 8, 2_000), &me, 1_399, false)).contains("outside the accepted window"));

        let weak = search(&keys, vec![], |e| leading_zero_bits(&e.id.to_bytes()) < 8);
        assert!(msg(guard.check(&weak, &me, 1_000, false)).contains("8 bits of NIP-13 work required"));
    }

    #[test]
    fn requires_the_event_to_be_a_request_for_this_service() {
        let keys = Keys::generate();
        let me = keys.public_key().to_hex();
        let guard = guard(0, 600, 100);
        let build = |kind: u16, tags: Vec<Tag>| {
            EventBuilder::new(Kind::Custom(kind), "{}")
                .tags(tags)
                .custom_created_at(Timestamp::from(1_000))
                .sign_with_keys(&keys)
                .unwrap()
        };
        let elsewhere = Tag::parse(["p", &"ab".repeat(32)]).unwrap();
        // Signed for another service, e.g. captured there and re-wrapped to us.
        for ev in [
            build(CTXVM_KIND, vec![elsewhere]),
            build(CTXVM_KIND, vec![]),
            build(CTXVM_KIND, vec![Tag::parse(["e", SERVICE]).unwrap()]),
            build(CTXVM_KIND, vec![Tag::parse(["p"]).unwrap()]),
            build(1, vec![to_service()]),
        ] {
            assert!(msg(guard.check(&ev, &me, 1_000, true)).contains("not a request addressed to this service"));
        }
        guard
            .check(
                &build(CTXVM_KIND, vec![Tag::parse(["e", "x"]).unwrap(), to_service()]),
                &me,
                1_000,
                true,
            )
            .unwrap();
    }

    #[test]
    fn operators_owe_no_work_but_cannot_replay_or_send_stale_requests() {
        let keys = Keys::generate();
        let me = keys.public_key().to_hex();
        let guard = guard(30, 600, 100);
        let ev = mined(&keys, 0, 1_000);
        assert!(msg(guard.check(&ev, &me, 1_000, false)).contains("30 bits"));
        guard.check(&ev, &me, 1_000, true).unwrap();
        assert!(msg(guard.check(&ev, &me, 1_000, true)).contains("already used"));
        assert!(msg(guard.check(&mined(&keys, 0, 1_000), &me, 9_000, true)).contains("outside the accepted window"));
    }

    #[test]
    fn requires_a_committed_target() {
        let keys = Keys::generate();
        let me = keys.public_key().to_hex();
        let guard = guard(8, 600, 100);
        // Enough leading zeros by luck, but without (or with a lower) commitment.
        let lucky = |tags: Vec<Tag>| search(&keys, tags, |e| leading_zero_bits(&e.id.to_bytes()) >= 8);
        let want = "commit to at least 8 bits";
        assert!(msg(guard.check(&lucky(vec![]), &me, 1_000, false)).contains(want));
        for nonce in [vec!["nonce", "1", "4"], vec!["nonce", "1", "lots"], vec!["nonce", "1"]] {
            let tag = Tag::parse(nonce).unwrap();
            assert!(msg(guard.check(&lucky(vec![tag]), &me, 1_000, false)).contains(want));
        }
        let ok = Tag::parse(["nonce", "1", "8"]).unwrap();
        guard.check(&lucky(vec![ok]), &me, 1_000, false).unwrap();
    }

    #[test]
    fn rejects_forged_signature() {
        let keys = Keys::generate();
        let me = keys.public_key().to_hex();
        let guard = guard(0, 600, 100);
        let good = mined(&keys, 0, 1_000);
        let mut json: serde_json::Value = serde_json::to_value(&good).unwrap();
        json["sig"] = serde_json::Value::String("00".repeat(64));
        let forged: Event = serde_json::from_value(json).unwrap();
        assert!(msg(guard.check(&forged, &me, 1_000, false)).contains("invalid event id or signature"));
    }

    #[test]
    fn zero_difficulty_still_blocks_replays_and_cache_is_bounded() {
        let keys = Keys::generate();
        let me = keys.public_key().to_hex();
        let guard = guard(0, 100, 2);
        let a = mined(&keys, 0, 1_000);
        guard.check(&a, &me, 1_000, false).unwrap();
        assert!(guard.check(&a, &me, 1_000, false).is_err());
        guard.check(&mined(&keys, 0, 1_001), &me, 1_000, false).unwrap();
        // Full, and nothing has aged out yet.
        assert!(matches!(
            guard.check(&mined(&keys, 0, 1_002), &me, 1_000, false),
            Err(Error::Capacity)
        ));
        // Later the old entries are forgotten and there is room again.
        guard.check(&mined(&keys, 0, 1_300), &me, 1_300, false).unwrap();
        assert_eq!(guard.seen.lock().unwrap().len(), 1);
    }

    #[test]
    fn survives_a_poisoned_lock() {
        let keys = Keys::generate();
        let guard = std::sync::Arc::new(guard(0, 100, 10));
        let g2 = guard.clone();
        let _ = std::thread::spawn(move || {
            let _held = g2.seen.lock().unwrap();
            panic!("poison");
        })
        .join();
        guard
            .check(&mined(&keys, 0, 1_000), &keys.public_key().to_hex(), 1_000, false)
            .unwrap();
    }
}
