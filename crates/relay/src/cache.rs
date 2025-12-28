//! Request cache for destination verification
//!
//! Caches request_id → user_pubkey mappings to verify response destinations.

use std::collections::HashMap;
use std::time::{Duration, Instant};
use tunnelcraft_core::{Id, PublicKey};

/// Default TTL for cached entries (5 minutes)
const DEFAULT_TTL: Duration = Duration::from_secs(300);

/// Maximum cache size
const DEFAULT_MAX_SIZE: usize = 10000;

/// A cached request entry
struct CacheEntry {
    user_pubkey: PublicKey,
    created_at: Instant,
}

/// LRU cache for request → user_pubkey mappings
///
/// Used by relays to verify that response destinations match the original requester.
pub struct RequestCache {
    entries: HashMap<Id, CacheEntry>,
    ttl: Duration,
    max_size: usize,
}

impl RequestCache {
    /// Create a new request cache with default settings
    pub fn new() -> Self {
        Self {
            entries: HashMap::new(),
            ttl: DEFAULT_TTL,
            max_size: DEFAULT_MAX_SIZE,
        }
    }

    /// Create a cache with custom TTL and max size
    pub fn with_config(ttl: Duration, max_size: usize) -> Self {
        Self {
            entries: HashMap::new(),
            ttl,
            max_size,
        }
    }

    /// Store a request_id → user_pubkey mapping
    pub fn insert(&mut self, request_id: Id, user_pubkey: PublicKey) {
        // Evict expired entries if at capacity
        if self.entries.len() >= self.max_size {
            self.evict_expired();
        }

        // If still at capacity, remove oldest entry
        if self.entries.len() >= self.max_size {
            self.evict_oldest();
        }

        self.entries.insert(
            request_id,
            CacheEntry {
                user_pubkey,
                created_at: Instant::now(),
            },
        );
    }

    /// Get the user_pubkey for a request_id
    pub fn get(&self, request_id: &Id) -> Option<PublicKey> {
        self.entries.get(request_id).and_then(|entry| {
            if entry.created_at.elapsed() < self.ttl {
                Some(entry.user_pubkey)
            } else {
                None
            }
        })
    }

    /// Check if a request_id exists and is not expired
    pub fn contains(&self, request_id: &Id) -> bool {
        self.get(request_id).is_some()
    }

    /// Remove a request_id from the cache
    pub fn remove(&mut self, request_id: &Id) -> Option<PublicKey> {
        self.entries.remove(request_id).map(|e| e.user_pubkey)
    }

    /// Get the number of entries in the cache
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Check if the cache is empty
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Remove all expired entries
    pub fn evict_expired(&mut self) {
        let now = Instant::now();
        self.entries
            .retain(|_, entry| now.duration_since(entry.created_at) < self.ttl);
    }

    /// Remove the oldest entry
    fn evict_oldest(&mut self) {
        if let Some(oldest_key) = self
            .entries
            .iter()
            .min_by_key(|(_, entry)| entry.created_at)
            .map(|(k, _)| *k)
        {
            self.entries.remove(&oldest_key);
        }
    }

    /// Clear all entries
    pub fn clear(&mut self) {
        self.entries.clear();
    }
}

impl Default for RequestCache {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_id(n: u8) -> Id {
        let mut id = [0u8; 32];
        id[0] = n;
        id
    }

    fn test_pubkey(n: u8) -> PublicKey {
        let mut pk = [0u8; 32];
        pk[0] = n;
        pk
    }

    #[test]
    fn test_insert_and_get() {
        let mut cache = RequestCache::new();
        let request_id = test_id(1);
        let user_pubkey = test_pubkey(1);

        cache.insert(request_id, user_pubkey);

        assert_eq!(cache.get(&request_id), Some(user_pubkey));
        assert!(cache.contains(&request_id));
    }

    #[test]
    fn test_missing_entry() {
        let cache = RequestCache::new();
        let request_id = test_id(1);

        assert_eq!(cache.get(&request_id), None);
        assert!(!cache.contains(&request_id));
    }

    #[test]
    fn test_remove() {
        let mut cache = RequestCache::new();
        let request_id = test_id(1);
        let user_pubkey = test_pubkey(1);

        cache.insert(request_id, user_pubkey);
        assert!(cache.contains(&request_id));

        let removed = cache.remove(&request_id);
        assert_eq!(removed, Some(user_pubkey));
        assert!(!cache.contains(&request_id));
    }

    #[test]
    fn test_max_size_eviction() {
        let mut cache = RequestCache::with_config(DEFAULT_TTL, 3);

        cache.insert(test_id(1), test_pubkey(1));
        cache.insert(test_id(2), test_pubkey(2));
        cache.insert(test_id(3), test_pubkey(3));
        assert_eq!(cache.len(), 3);

        // Adding 4th should evict oldest
        cache.insert(test_id(4), test_pubkey(4));
        assert_eq!(cache.len(), 3);

        // Entry 4 should exist
        assert!(cache.contains(&test_id(4)));
    }

    #[test]
    fn test_expired_entry() {
        let mut cache = RequestCache::with_config(Duration::from_millis(10), 100);
        let request_id = test_id(1);
        let user_pubkey = test_pubkey(1);

        cache.insert(request_id, user_pubkey);
        assert!(cache.contains(&request_id));

        // Wait for expiration
        std::thread::sleep(Duration::from_millis(20));

        // Should be expired now
        assert!(!cache.contains(&request_id));
        assert_eq!(cache.get(&request_id), None);
    }

    #[test]
    fn test_clear() {
        let mut cache = RequestCache::new();
        cache.insert(test_id(1), test_pubkey(1));
        cache.insert(test_id(2), test_pubkey(2));
        assert_eq!(cache.len(), 2);

        cache.clear();
        assert!(cache.is_empty());
    }
}
