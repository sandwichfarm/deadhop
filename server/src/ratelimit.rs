//! Fixed-window rate limiter for public invoice creation.
//!
//! Memory is bounded by construction: a key only gets an entry when the global
//! budget still has room, so at most `global_per_min` keys exist per window.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Mutex;

struct Window {
    minute: u64,
    global: u32,
    keys: HashMap<String, u32>,
}

/// Per-key and global request budget per minute.
pub struct RateLimiter {
    global_per_min: AtomicU32,
    per_key_per_min: AtomicU32,
    window: Mutex<Window>,
}

impl RateLimiter {
    pub fn new(global_per_min: u32, per_key_per_min: u32) -> Self {
        RateLimiter {
            global_per_min: AtomicU32::new(global_per_min),
            per_key_per_min: AtomicU32::new(per_key_per_min),
            window: Mutex::new(Window {
                minute: 0,
                global: 0,
                keys: HashMap::new(),
            }),
        }
    }

    /// Change both budgets; takes effect immediately.
    pub fn set_limits(&self, global_per_min: u32, per_key_per_min: u32) {
        self.global_per_min.store(global_per_min, Ordering::Relaxed);
        self.per_key_per_min.store(per_key_per_min, Ordering::Relaxed);
    }

    fn decide(&self, key: &str, now: u64, consume: bool) -> bool {
        let mut w = self.window.lock().unwrap_or_else(|p| p.into_inner());
        let minute = now / 60;
        if w.minute != minute {
            w.minute = minute;
            w.global = 0;
            w.keys.clear();
        }
        if w.global >= self.global_per_min.load(Ordering::Relaxed) {
            return false;
        }
        let used = w.keys.get(key).copied().unwrap_or(0);
        if used >= self.per_key_per_min.load(Ordering::Relaxed) {
            return false;
        }
        if consume {
            w.keys.insert(key.to_string(), used + 1);
            w.global += 1;
        }
        true
    }

    /// Consume one unit for `key` at unix time `now`. `false` means refuse.
    pub fn allow(&self, key: &str, now: u64) -> bool {
        self.decide(key, now, true)
    }

    /// Whether `allow` would currently succeed, without consuming anything.
    pub fn would_allow(&self, key: &str, now: u64) -> bool {
        self.decide(key, now, false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn per_key_and_global_budgets() {
        let rl = RateLimiter::new(5, 2);
        assert!(rl.would_allow("a", 60));
        assert!(rl.would_allow("a", 60), "checking consumes nothing");
        assert!(rl.allow("a", 60));
        assert!(rl.allow("a", 61));
        assert!(!rl.would_allow("a", 62));
        assert!(!rl.allow("a", 62), "key budget spent");
        assert!(rl.allow("b", 62));
        assert!(rl.allow("b", 63));
        assert!(rl.allow("c", 64));
        assert!(!rl.would_allow("d", 65));
        assert!(!rl.allow("d", 65), "global budget spent");
        assert_eq!(rl.window.lock().unwrap().keys.len(), 3, "refused keys take no memory");
        // Next minute: everything resets.
        assert!(rl.allow("a", 120));
        assert!(rl.allow("d", 121));
        assert_eq!(rl.window.lock().unwrap().keys.len(), 2);
    }

    #[test]
    fn survives_a_poisoned_lock() {
        let rl = std::sync::Arc::new(RateLimiter::new(5, 2));
        let r2 = rl.clone();
        let _ = std::thread::spawn(move || {
            let _held = r2.window.lock().unwrap();
            panic!("poison");
        })
        .join();
        assert!(rl.allow("a", 60));
    }
}
