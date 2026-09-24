use std::time::Duration;
use rtdp_core::buffers::BoundedRingBuffer;
use rtdp_core::delivery::{DeliveryMode, StreamReceiver};
use rtdp_core::error::ErrorCode;
use rtdp_core::protocol::frame::OwnedFrame;
use rtdp_core::recovery::{RecoveryConfig, RecoveryEngine, RecoveryState};
use rtdp_core::sequence::{DuplicateFilter, DuplicateKey, SequenceStatus, StreamSequenceTracker};

#[test]
fn test_ring_buffer_retention_and_expiry() {
    let mut ring = BoundedRingBuffer::new(5);

    for seq in 1..=5 {
        let frame = OwnedFrame::new_data(1, seq, 1000, 10, format!("payload-{}", seq).into_bytes());
        ring.push(frame);
    }

    assert_eq!(ring.min_sequence(), Some(1));
    assert_eq!(ring.max_sequence(), Some(5));

    // Push 3 more frames (seq 6, 7, 8). Capacity is 5, so min should advance to 4
    for seq in 6..=8 {
        let frame = OwnedFrame::new_data(1, seq, 1000, 10, format!("payload-{}", seq).into_bytes());
        ring.push(frame);
    }

    assert_eq!(ring.min_sequence(), Some(4));
    assert_eq!(ring.max_sequence(), Some(8));

    // Sequence 1, 2, 3 should be expired (REPLAY_UNAVAILABLE)
    let range_res = ring.get_range(2, 6);
    assert!(range_res.is_err());
    let (err, unavail_from, unavail_to) = range_res.unwrap_err();
    assert_eq!(err, ErrorCode::ReplayUnavailable);
    assert_eq!(unavail_from, 2);
    assert_eq!(unavail_to, 3);

    // Sequence 4 to 8 should be available
    let valid_range = ring.get_range(4, 7).expect("Sequences 4..7 available");
    assert_eq!(valid_range.len(), 4);
    assert_eq!(valid_range[0].header.sequence, 4);
    assert_eq!(valid_range[3].header.sequence, 7);
}

#[test]
fn test_sequence_tracking_and_publisher_restart() {
    let mut tracker = StreamSequenceTracker::new(42);

    // Initial sequence
    let s1 = tracker.observe(100, 1);
    assert_eq!(s1, SequenceStatus::InOrder { sequence: 1 });

    // Next in order
    let s2 = tracker.observe(100, 2);
    assert_eq!(s2, SequenceStatus::InOrder { sequence: 2 });

    // Duplicate
    let s_dup = tracker.observe(100, 1);
    assert_eq!(
        s_dup,
        SequenceStatus::DuplicateOrOld {
            sequence: 1,
            expected: 3
        }
    );

    // Gap (expected 3, got 6)
    let s_gap = tracker.observe(100, 6);
    assert_eq!(
        s_gap,
        SequenceStatus::Gap {
            expected: 3,
            received: 6,
            from: 3,
            to: 5
        }
    );

    // Publisher restart with new publisher_id 101 resetting sequence to 1
    let s_reset = tracker.observe(101, 1);
    assert_eq!(
        s_reset,
        SequenceStatus::PublisherReset {
            old_publisher: Some(100),
            new_publisher: 101,
            initial_sequence: 1
        }
    );
    assert_eq!(tracker.total_publisher_resets, 1);
}

#[test]
fn test_duplicate_filter() {
    let mut filter = DuplicateFilter::new(10);
    let key1 = DuplicateKey::new(1, 10, 1);
    let key2 = DuplicateKey::new(1, 10, 2);

    assert!(!filter.is_duplicate_and_record(key1));
    assert!(filter.is_duplicate_and_record(key1));
    assert!(!filter.is_duplicate_and_record(key2));
    assert!(filter.is_duplicate_and_record(key2));
}

#[test]
fn test_recovery_engine_retries_and_idempotence() {
    let mut engine = RecoveryEngine::new(RecoveryConfig {
        retry_timeout: Duration::from_millis(10),
        max_retries: 2,
        max_replay_range: 50,
        max_outstanding_requests: 4,
    });

    let req = engine.create_replay_request(10, 12).expect("Created request");
    assert_eq!(req.request_id, 1);
    assert_eq!(req.from_sequence, 10);
    assert_eq!(req.to_sequence, 12);
    assert_eq!(engine.state(), RecoveryState::Recovering);

    // Sleep past retry timeout
    std::thread::sleep(Duration::from_millis(15));
    let retries = engine.check_timeouts().expect("Check timeouts");
    assert_eq!(retries.len(), 1);
    assert_eq!(retries[0].request_id, 1); // Same request_id retried

    // Receive replay responses
    let f10 = OwnedFrame::new_data(1, 10, 1000, 1, b"DATA-10".to_vec());
    let f11 = OwnedFrame::new_data(1, 11, 1000, 1, b"DATA-11".to_vec());
    let f12 = OwnedFrame::new_data(1, 12, 1000, 1, b"DATA-12".to_vec());

    assert!(engine.handle_replay_response(1, f10).unwrap().is_none());
    assert!(engine.handle_replay_response(1, f11).unwrap().is_none());
    let drained = engine.handle_replay_response(1, f12).unwrap();
    assert!(drained.is_some());
    let frames = drained.unwrap();
    assert_eq!(frames.len(), 3);
    assert_eq!(frames[0].header.sequence, 10);
    assert_eq!(frames[1].header.sequence, 11);
    assert_eq!(frames[2].header.sequence, 12);

    assert_eq!(engine.state(), RecoveryState::Live);
}

#[test]
fn test_delivery_modes() {
    // 1. BEST_EFFORT
    let mut rx_be = StreamReceiver::new(1, DeliveryMode::BestEffort, RecoveryConfig::default());
    let f1 = OwnedFrame::new_data(1, 1, 1000, 10, b"1".to_vec());
    let f3 = OwnedFrame::new_data(1, 3, 1000, 10, b"3".to_vec());
    let (d1, r1) = rx_be.process_data_frame(f1).unwrap();
    assert_eq!(d1.len(), 1);
    assert!(r1.is_none());
    let (d3, r3) = rx_be.process_data_frame(f3).unwrap();
    assert_eq!(d3.len(), 1); // Delivered immediately, no replay requested
    assert!(r3.is_none());

    // 2. RELIABLE_ORDERED
    let mut rx_ro = StreamReceiver::new(1, DeliveryMode::ReliableOrdered, RecoveryConfig::default());
    let f1 = OwnedFrame::new_data(1, 1, 1000, 10, b"1".to_vec());
    let f3 = OwnedFrame::new_data(1, 3, 1000, 10, b"3".to_vec());
    let (d1, r1) = rx_ro.process_data_frame(f1).unwrap();
    assert_eq!(d1.len(), 1);
    assert!(r1.is_none());

    // Packet 3 arrives when packet 2 was missing: should buffer packet 3 and request replay for 2
    let (d3, r3) = rx_ro.process_data_frame(f3).unwrap();
    assert_eq!(d3.len(), 0); // Withheld from application
    assert!(r3.is_some());
    let req = r3.unwrap();
    assert_eq!(req.from_sequence, 2);
    assert_eq!(req.to_sequence, 2);
}
