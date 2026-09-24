use crate::error::RtdpError;
use crate::protocol::frame::OwnedFrame;
use crate::recovery::{RecoveryConfig, RecoveryEngine, ReplayRequestPayload};
use crate::sequence::{DuplicateFilter, DuplicateKey, SequenceStatus, StreamSequenceTracker};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeliveryMode {
    BestEffort,
    Latest,
    AtLeastOnce,
    ReliableOrdered,
}

pub struct StreamReceiver {
    pub stream_id: u64,
    pub mode: DeliveryMode,
    pub tracker: StreamSequenceTracker,
    pub dedup: DuplicateFilter,
    pub recovery: RecoveryEngine,
    pub highest_delivered_sequence: u64,
}

impl StreamReceiver {
    pub fn new(stream_id: u64, mode: DeliveryMode, recovery_config: RecoveryConfig) -> Self {
        Self {
            stream_id,
            mode,
            tracker: StreamSequenceTracker::new(stream_id),
            dedup: DuplicateFilter::new(10_000),
            recovery: RecoveryEngine::new(recovery_config),
            highest_delivered_sequence: 0,
        }
    }

    /// Process an incoming DATA frame according to the selected DeliveryMode
    pub fn process_data_frame(
        &mut self,
        frame: OwnedFrame,
    ) -> Result<(Vec<OwnedFrame>, Option<ReplayRequestPayload>), RtdpError> {
        let pub_id = frame.header.publisher_id;
        let seq = frame.header.sequence;

        let status = self.tracker.observe(pub_id, seq);
        let key = DuplicateKey::new(self.stream_id, pub_id, seq);

        match self.mode {
            DeliveryMode::BestEffort => {
                // Deliver whatever arrives without gap recovery
                self.highest_delivered_sequence = self.highest_delivered_sequence.max(seq);
                Ok((vec![frame], None))
            }

            DeliveryMode::Latest => {
                // Only deliver if strictly newer than highest seen; suppress stale
                if seq > self.highest_delivered_sequence {
                    self.highest_delivered_sequence = seq;
                    Ok((vec![frame], None))
                } else {
                    // Stale data dropped locally
                    Ok((vec![], None))
                }
            }

            DeliveryMode::AtLeastOnce => {
                // Duplicates may be delivered to application, but gaps are requested for replay
                match status {
                    SequenceStatus::InOrder { sequence } => {
                        self.highest_delivered_sequence = self.highest_delivered_sequence.max(sequence);
                        Ok((vec![frame], None))
                    }
                    SequenceStatus::Gap { from, to, .. } => {
                        let replay_req = self.recovery.create_replay_request(from, to)?;
                        self.highest_delivered_sequence = self.highest_delivered_sequence.max(seq);
                        Ok((vec![frame], Some(replay_req)))
                    }
                    SequenceStatus::DuplicateOrOld { .. } => {
                        // In AT_LEAST_ONCE, duplicates pass through to consumer
                        Ok((vec![frame], None))
                    }
                    SequenceStatus::PublisherReset { initial_sequence, .. } => {
                        self.highest_delivered_sequence = initial_sequence;
                        Ok((vec![frame], None))
                    }
                }
            }

            DeliveryMode::ReliableOrdered => {
                // Suppress duplicates
                if self.dedup.is_duplicate_and_record(key) {
                    return Ok((vec![], None));
                }

                match status {
                    SequenceStatus::InOrder { sequence } => {
                        let mut delivered = vec![frame];
                        self.highest_delivered_sequence = sequence;

                        // Check if previously buffered out-of-order frames can now be delivered
                        let mut next_expected = sequence + 1;
                        let contiguous = self.recovery.drain_contiguous(next_expected);
                        for c in contiguous {
                            let c_seq = c.header.sequence;
                            self.highest_delivered_sequence = c_seq;
                            next_expected = c_seq + 1;
                            self.tracker.advance_expected(next_expected);
                            delivered.push(c);
                        }

                        Ok((delivered, None))
                    }
                    SequenceStatus::Gap { from, to, .. } => {
                        // Buffer out-of-order frame and request missing range
                        self.recovery.buffer_frame(frame);
                        let replay_req = self.recovery.create_replay_request(from, to)?;
                        Ok((vec![], Some(replay_req)))
                    }
                    SequenceStatus::DuplicateOrOld { .. } => {
                        // Suppressed
                        Ok((vec![], None))
                    }
                    SequenceStatus::PublisherReset { initial_sequence, .. } => {
                        self.recovery.reset();
                        self.highest_delivered_sequence = initial_sequence;
                        Ok((vec![frame], None))
                    }
                }
            }
        }
    }
}
