//! Mini key-value cache — demo project for the keplr showcase.
//! Open this file in keplr to see tree-sitter highlighting, then
//! hit the terminal tab and run `cargo test`.

use std::collections::HashMap;
use std::time::{Duration, Instant};

/// A tiny in-memory cache with per-key TTLs.
pub struct Cache {
    /// Map from key to (value, deadline).
    entries: HashMap<String, (String, Instant)>,
    default_ttl: Duration,
    hits: u64,
    misses: u64,
}

impl Cache {
    /// Create an empty cache. `ttl_secs` applies to every `insert`.
    pub fn new(ttl_secs: u64) -> Self {
        Self {
            entries: HashMap::new(),
            default_ttl: Duration::from_secs(ttl_secs),
            hits: 0,
            misses: 0,
        }
    }

    /// Store `value` under `key`, overwriting any previous entry.
    pub fn insert(&mut self, key: &str, value: &str) {
        let deadline = Instant::now() + self.default_ttl;
        self.entries
            .insert(key.to_string(), (value.to_string(), deadline));
    }

    /// Look a key up. Expired entries count as misses and are evicted.
    pub fn get(&mut self, key: &str) -> Option<String> {
        match self.entries.get(key) {
            Some((value, deadline)) if *deadline > Instant::now() => {
                self.hits += 1;
                Some(value.clone())
            }
            _ => {
                self.misses += 1;
                self.entries.remove(key);
                None
            }
        }
    }

    /// Fraction of lookups that hit, in `0.0..=1.0`.
    pub fn hit_rate(&self) -> f64 {
        let total = self.hits + self.misses;
        if total == 0 {
            0.0
        } else {
            self.hits as f64 / total as f64
        }
    }
}

fn main() {
    let mut cache = Cache::new(60);
    cache.insert("hello", "world");
    // Try hovering, searching, and splitting panes in keplr!
    println!("hit: {:?}", cache.get("hello"));
    println!("miss: {:?}", cache.get("nope"));
    println!("hit rate: {:.1}%", cache.hit_rate() * 100.0);
}
