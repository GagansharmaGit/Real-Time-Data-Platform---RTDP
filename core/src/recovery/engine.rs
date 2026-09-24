use std::collections::{BTreeMap, HashMap};
use std::time::{Duration, Instant};

use byteorder::{BigEndian, ByteOrder};
use crate::error::{ErrorCode, RtdpError};
use crate::protocol::frame::OwnedFrame;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReplayRequestPayload {
    pub request_id: u64,
    pub from_sequence: u64,
    pub to_sequence: u64,
}

impl ReplayRequestPayload {
    pub fn encode(&self, dst: &mut [u8]) -> Result<(), RtdpError> {
        if dst.len() < 24 {
            return Err(RtdpError::malformed("Buffer too short for REPLAY_REQUEST payload"));
        }
        BigEndian::write_u64(&mut dst[0..8], self.request_id);
        BigEndian::write_u64(&mut dst[8..16], self.from_sequence);
        BigEndian::write_u64(&mut dst[16..24], self.to_sequence);
        Ok(())
    }

    pub fn decode(src: &[u8]) -> Result<Self, RtdpError> {
        if src.len() < 24 {
            return Err(RtdpError::malformed("REPLAY_REQUEST payload < 24 bytes"));
        }
        let request_id = BigEndian::read_u64(&src[0..8]);
        let from_sequence = BigEndian::read_u64(&src[8..16]);
        let to_sequence = BigEndian::read_u64(&src[16..24]);

        if from_sequence > to_sequence {
            return Err(RtdpError::Protocol {
                code: ErrorCode::ReplayRangeInvalid,
                detail: format!("from_sequence {} > to_sequence {}", from_sequence, to_sequence),
            });
        }

        Ok(Self {
            request_id,
            from_sequence,
            to_sequence,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReplayUnavailablePayload {
    pub request_id: u64,
    pub first_unavailable: u64,
    pub last_unavailable: u64,
}

impl ReplayUnavailablePayload {
    pub fn encode(&self, dst: &mut [u8]) -> Result<(), RtdpError> {
        if dst.len() < 24 {
            return Err(RtdpError::malformed("Buffer too short for REPLAY_UNAVAILABLE payload"));
        }
        BigEndian::write_u64(&mut dst[0..8], self.request_id);
        BigEndian::write_u64(&mut dst[8..16], self.first_unavailable);
        BigEndian::write_u64(&mut dst[16..24], self.last_unavailable);
        Ok(())
    }

    pub fn decode(src: &[u8]) -> Result<Self, RtdpError> {
        if src.len() < 24 {
            return Err(RtdpError::malformed("REPLAY_UNAVAILABLE payload < 24 bytes"));
        }
        Ok(Self {
            request_id: BigEndian::read_u64(&src[0..8]),
            first_unavailable: BigEndian::read_u64(&src[8..16]),
            last_unavailable: BigEndian::read_u64(&src[16..24]),
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecoveryState {
    Live,
    Recovering,
    RecoveryFailed,
}

#[derive(Debug, Clone)]
pub struct ActiveReplayRequest {
    pub request_id: u64,
    pub from_sequence: u64,
    pub to_sequence: u64,
    pub created_at: Instant,
    pub last_sent_at: Instant,
    pub retries_attempted: usize,
    pub received_sequences: Vec<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RecoveryConfig {
    pub retry_timeout: Duration,
    pub max_retries: usize,
    pub max_replay_range: u64,
    pub max_outstanding_requests: usize,
}

impl Default for RecoveryConfig {
    fn default() -> Self {
        Self {
            retry_timeout: Duration::from_millis(50),
            max_retries: 3,
            max_replay_range: 1000,
            max_outstanding_requests: 16,
        }
    }
}

pub struct RecoveryEngine {
    config: RecoveryConfig,
    state: RecoveryState,
    next_request_id: u64,
    outstanding_requests: HashMap<u64, ActiveReplayRequest>,
    buffered_out_of_order: BTreeMap<u64, OwnedFrame>,
}

impl RecoveryEngine {
    pub fn new(config: RecoveryConfig) -> Self {
        Self {
            config,
            state: RecoveryState::Live,
            next_request_id: 1,
            outstanding_requests: HashMap::new(),
            buffered_out_of_order: BTreeMap::new(),
        }
    }

    pub fn state(&self) -> RecoveryState {
        self.state
    }

    /// Buffer an out-of-order frame while waiting for gap resolution
    pub fn buffer_frame(&mut self, frame: OwnedFrame) {
        self.buffered_out_of_order
            .insert(frame.header.sequence, frame);
    }

    /// Create a new replay request for a missing sequence range
    pub fn create_replay_request(
        &mut self,
        from_sequence: u64,
        to_sequence: u64,
    ) -> Result<ReplayRequestPayload, RtdpError> {
        let range_len = to_sequence.saturating_sub(from_sequence) + 1;
        if range_len > self.config.max_replay_range {
            return Err(RtdpError::Protocol {
                code: ErrorCode::ReplayRangeInvalid,
                detail: format!(
                    "Requested range {} exceeds maximum allowed {}",
                    range_len, self.config.max_replay_range
                ),
            });
        }

        if self.outstanding_requests.len() >= self.config.max_outstanding_requests {
            return Err(RtdpError::Protocol {
                code: ErrorCode::Backpressure,
                detail: "Exceeded max outstanding replay requests".into(),
            });
        }

        let request_id = self.next_request_id;
        self.next_request_id += 1;

        let now = Instant::now();
        let req = ActiveReplayRequest {
            request_id,
            from_sequence,
            to_sequence,
            created_at: now,
            last_sent_at: now,
            retries_attempted: 0,
            received_sequences: Vec::new(),
        };

        self.outstanding_requests.insert(request_id, req);
        self.state = RecoveryState::Recovering;

        Ok(ReplayRequestPayload {
            request_id,
            from_sequence,
            to_sequence,
        })
    }

    /// Handle incoming REPLAY_RESPONSE
    pub fn handle_replay_response(
        &mut self,
        request_id: u64,
        frame: OwnedFrame,
    ) -> Result<Option<Vec<OwnedFrame>>, RtdpError> {
        let seq = frame.header.sequence;

        // Verify request exists
        if let Some(req) = self.outstanding_requests.get_mut(&request_id) {
            if seq >= req.from_sequence && seq <= req.to_sequence {
                if !req.received_sequences.contains(&seq) {
                    req.received_sequences.push(seq);
                }
            }

            // Buffer this frame so it can be ordered
            self.buffered_out_of_order.insert(seq, frame);

            // Check if full requested range has arrived
            let mut all_arrived = true;
            for s in req.from_sequence..=req.to_sequence {
                if !req.received_sequences.contains(&s) {
                    all_arrived = false;
                    break;
                }
            }

            if all_arrived {
                self.outstanding_requests.remove(&request_id);
                if self.outstanding_requests.is_empty() {
                    self.state = RecoveryState::Live;
                }

                // Drain all contiguous buffered frames
                return Ok(Some(self.drain_buffered()));
            }
        }

        Ok(None)
    }

    /// Handle incoming REPLAY_UNAVAILABLE: transition to error state
    pub fn handle_replay_unavailable(
        &mut self,
        unavail: &ReplayUnavailablePayload,
    ) -> Result<(), RtdpError> {
        self.outstanding_requests.remove(&unavail.request_id);
        self.state = RecoveryState::RecoveryFailed;
        Err(RtdpError::UnrecoverableGap {
            from: unavail.first_unavailable,
            to: unavail.last_unavailable,
        })
    }

    /// Check timeouts and return requests that need re-transmission
    pub fn check_timeouts(&mut self) -> Result<Vec<ReplayRequestPayload>, RtdpError> {
        let mut retries = Vec::new();
        let mut failed = Vec::new();
        let now = Instant::now();

        for (req_id, req) in self.outstanding_requests.iter_mut() {
            if now.duration_since(req.last_sent_at) >= self.config.retry_timeout {
                if req.retries_attempted >= self.config.max_retries {
                    failed.push((*req_id, req.from_sequence, req.to_sequence));
                } else {
                    req.retries_attempted += 1;
                    req.last_sent_at = now;
                    retries.push(ReplayRequestPayload {
                        request_id: *req_id,
                        from_sequence: req.from_sequence,
                        to_sequence: req.to_sequence,
                    });
                }
            }
        }

        for (req_id, from_s, to_s) in failed {
            self.outstanding_requests.remove(&req_id);
            self.state = RecoveryState::RecoveryFailed;
            return Err(RtdpError::UnrecoverableGap {
                from: from_s,
                to: to_s,
            });
        }

        Ok(retries)
    }

    /// Drain all contiguous buffered frames starting from `start_seq`
    pub fn drain_contiguous(&mut self, start_seq: u64) -> Vec<OwnedFrame> {
        let mut current = start_seq;
        let mut res = Vec::new();
        while let Some(frame) = self.buffered_out_of_order.remove(&current) {
            res.push(frame);
            current += 1;
        }
        res
    }

    /// Drain all contiguous buffered frames
    pub fn drain_buffered(&mut self) -> Vec<OwnedFrame> {
        let frames: Vec<OwnedFrame> = self.buffered_out_of_order.values().cloned().collect();
        self.buffered_out_of_order.clear();
        frames
    }

    pub fn reset(&mut self) {
        self.state = RecoveryState::Live;
        self.outstanding_requests.clear();
        self.buffered_out_of_order.clear();
    }
}
