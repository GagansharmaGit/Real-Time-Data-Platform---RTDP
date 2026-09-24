use crate::error::ErrorCode;
use crate::protocol::frame::OwnedFrame;

/// Bounded ring buffer for retaining published DATA frames for replay
pub struct BoundedRingBuffer {
    capacity: usize,
    buffer: Vec<Option<OwnedFrame>>,
    min_sequence: Option<u64>,
    max_sequence: Option<u64>,
}

impl BoundedRingBuffer {
    pub fn new(capacity: usize) -> Self {
        assert!(capacity > 0, "Capacity must be positive");
        let mut buffer = Vec::with_capacity(capacity);
        for _ in 0..capacity {
            buffer.push(None);
        }

        Self {
            capacity,
            buffer,
            min_sequence: None,
            max_sequence: None,
        }
    }

    pub fn capacity(&self) -> usize {
        self.capacity
    }

    pub fn min_sequence(&self) -> Option<u64> {
        self.min_sequence
    }

    pub fn max_sequence(&self) -> Option<u64> {
        self.max_sequence
    }

    /// Store a published DATA frame in the ring buffer
    pub fn push(&mut self, frame: OwnedFrame) {
        let seq = frame.header.sequence;
        let index = (seq as usize) % self.capacity;

        self.buffer[index] = Some(frame);

        match self.min_sequence {
            None => {
                self.min_sequence = Some(seq);
                self.max_sequence = Some(seq);
            }
            Some(min_s) => {
                self.max_sequence = Some(seq);
                // If we've wrapped around capacity, the oldest retained sequence advances
                if seq > min_s && (seq - min_s) >= self.capacity as u64 {
                    self.min_sequence = Some(seq - self.capacity as u64 + 1);
                }
            }
        }
    }

    /// Retrieve a single frame by sequence, returning None if expired or not yet published
    pub fn get(&self, sequence: u64) -> Option<&OwnedFrame> {
        let min_s = self.min_sequence?;
        let max_s = self.max_sequence?;

        if sequence < min_s || sequence > max_s {
            return None;
        }

        let index = (sequence as usize) % self.capacity;
        if let Some(frame) = &self.buffer[index] {
            if frame.header.sequence == sequence {
                return Some(frame);
            }
        }
        None
    }

    /// Validate and fetch range `[from_sequence, to_sequence]`.
    /// Returns `Err(ErrorCode::ReplayUnavailable)` if any requested sequence is outside retention.
    pub fn get_range(
        &self,
        from_seq: u64,
        to_seq: u64,
    ) -> Result<Vec<OwnedFrame>, (ErrorCode, u64, u64)> {
        let min_s = self.min_sequence.unwrap_or(0);
        let max_s = self.max_sequence.unwrap_or(0);

        if from_seq < min_s {
            // First unavailable sequence is from_seq, last unavailable is min_s - 1
            return Err((ErrorCode::ReplayUnavailable, from_seq, min_s.saturating_sub(1)));
        }

        if to_seq > max_s {
            return Err((ErrorCode::ReplayUnavailable, max_s + 1, to_seq));
        }

        let mut result = Vec::with_capacity((to_seq - from_seq + 1) as usize);
        for seq in from_seq..=to_seq {
            match self.get(seq) {
                Some(f) => result.push(f.clone()),
                None => {
                    return Err((ErrorCode::ReplayUnavailable, seq, seq));
                }
            }
        }

        Ok(result)
    }

    pub fn clear(&mut self) {
        for slot in &mut self.buffer {
            *slot = None;
        }
        self.min_sequence = None;
        self.max_sequence = None;
    }
}
