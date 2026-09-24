#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SequenceStatus {
    /// Packet arrived in exact expected order
    InOrder {
        sequence: u64,
    },
    /// Gap detected: range is inclusive `[from, to]`
    Gap {
        expected: u64,
        received: u64,
        from: u64,
        to: u64,
    },
    /// Duplicate or out-of-order older packet
    DuplicateOrOld {
        sequence: u64,
        expected: u64,
    },
    /// Publisher namespace changed (publisher restarted with new identity)
    PublisherReset {
        old_publisher: Option<u64>,
        new_publisher: u64,
        initial_sequence: u64,
    },
}

pub struct StreamSequenceTracker {
    pub stream_id: u64,
    pub active_publisher_id: Option<u64>,
    pub expected_sequence: u64,
    pub total_gaps_detected: u64,
    pub total_duplicates_detected: u64,
    pub total_publisher_resets: u64,
}

impl StreamSequenceTracker {
    pub fn new(stream_id: u64) -> Self {
        Self {
            stream_id,
            active_publisher_id: None,
            expected_sequence: 1, // Sequences start at 1
            total_gaps_detected: 0,
            total_duplicates_detected: 0,
            total_publisher_resets: 0,
        }
    }

    /// Process incoming publication sequence under Section 6 rules
    pub fn observe(&mut self, publisher_id: u64, sequence: u64) -> SequenceStatus {
        match self.active_publisher_id {
            None => {
                // Initial publisher registration; sequence starts at 1
                self.active_publisher_id = Some(publisher_id);
                if sequence == 1 {
                    self.expected_sequence = 2;
                    SequenceStatus::InOrder { sequence: 1 }
                } else {
                    let from = 1;
                    let to = sequence - 1;
                    self.expected_sequence = 1;
                    self.total_gaps_detected += 1;
                    SequenceStatus::Gap {
                        expected: 1,
                        received: sequence,
                        from,
                        to,
                    }
                }
            }
            Some(active_pub) if active_pub != publisher_id => {
                // Section 6: Publisher restart that resets sequence uses new publisher_id.
                // Receiver must not assume continuity across publisher_id changes.
                let old_pub = self.active_publisher_id;
                self.active_publisher_id = Some(publisher_id);
                self.total_publisher_resets += 1;

                if sequence == 1 {
                    self.expected_sequence = 2;
                    SequenceStatus::PublisherReset {
                        old_publisher: old_pub,
                        new_publisher: publisher_id,
                        initial_sequence: 1,
                    }
                } else {
                    self.expected_sequence = 1;
                    let from = 1;
                    let to = sequence - 1;
                    self.total_gaps_detected += 1;
                    SequenceStatus::Gap {
                        expected: 1,
                        received: sequence,
                        from,
                        to,
                    }
                }
            }
            Some(_) => {
                if sequence == self.expected_sequence {
                    self.expected_sequence += 1;
                    SequenceStatus::InOrder { sequence }
                } else if sequence < self.expected_sequence {
                    self.total_duplicates_detected += 1;
                    SequenceStatus::DuplicateOrOld {
                        sequence,
                        expected: self.expected_sequence,
                    }
                } else {
                    let from = self.expected_sequence;
                    let to = sequence - 1;
                    self.total_gaps_detected += 1;
                    SequenceStatus::Gap {
                        expected: self.expected_sequence,
                        received: sequence,
                        from,
                        to,
                    }
                }
            }
        }
    }

    /// Advance expected sequence directly (e.g. after replay resolution)
    pub fn advance_expected(&mut self, new_expected: u64) {
        if new_expected > self.expected_sequence {
            self.expected_sequence = new_expected;
        }
    }
}
