use std::collections::HashSet;

/// Consumer duplicate key: (stream_id, publisher_id, sequence)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DuplicateKey {
    pub stream_id: u64,
    pub publisher_id: u64,
    pub sequence: u64,
}

impl DuplicateKey {
    pub fn new(stream_id: u64, publisher_id: u64, sequence: u64) -> Self {
        Self {
            stream_id,
            publisher_id,
            sequence,
        }
    }
}

/// Deduplication filter with bounded memory
pub struct DuplicateFilter {
    recent_keys: HashSet<DuplicateKey>,
    ring_history: Vec<DuplicateKey>,
    head: usize,
    capacity: usize,
}

impl DuplicateFilter {
    pub fn new(capacity: usize) -> Self {
        Self {
            recent_keys: HashSet::with_capacity(capacity),
            ring_history: Vec::with_capacity(capacity),
            head: 0,
            capacity,
        }
    }

    /// Check if key is a duplicate. If not duplicate, record it and return false.
    pub fn is_duplicate_and_record(&mut self, key: DuplicateKey) -> bool {
        if self.recent_keys.contains(&key) {
            return true;
        }

        if self.ring_history.len() < self.capacity {
            self.ring_history.push(key);
            self.recent_keys.insert(key);
        } else {
            // Evict oldest from ring
            let old_key = self.ring_history[self.head];
            self.recent_keys.remove(&old_key);
            self.ring_history[self.head] = key;
            self.recent_keys.insert(key);
            self.head = (self.head + 1) % self.capacity;
        }

        false
    }

    pub fn clear(&mut self) {
        self.recent_keys.clear();
        self.ring_history.clear();
        self.head = 0;
    }
}
