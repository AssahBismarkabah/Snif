use std::collections::HashMap;
use std::time::{Duration, Instant};

/// A simple in-memory cache with time-based expiration.
pub struct Cache {
    entries: HashMap<String, (String, Instant)>,
    ttl: Duration,
}

impl Cache {
    /// Creates a new cache with the given time-to-live for entries.
    pub fn new(ttl: Duration) -> Self {
        Self {
            entries: HashMap::new(),
            ttl,
        }
    }

    /// Retrieves a value from the cache if it exists and has not expired.
    pub fn get(&self, key: &str) -> Option<&str> {
        self.entries.get(key).and_then(|(val, inserted)| {
            if inserted.elapsed() < self.ttl {
                Some(val.as_str())
            } else {
                None
            }
        })
    }

    /// Inserts a key-value pair into the cache.
    pub fn set(&mut self, key: String, value: String) {
        self.entries.insert(key, (value, Instant::now()));
    }
}
