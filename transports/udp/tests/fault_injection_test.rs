use std::net::SocketAddr;
use std::time::Duration;

use rtdp_core::delivery::DeliveryMode;
use rtdp_core::error::ErrorCode;
use rtdp_core::protocol::frame::OwnedFrame;
use rtdp_core::recovery::{RecoveryConfig, RecoveryEngine, RecoveryState, ReplayUnavailablePayload};
use rtdp_core::serialization::{CodecConfig, FrameCodec};
use rtdp_transport_udp::{UdpPublisher, UdpPublisherConfig, UdpSubscriber, UdpSubscriberConfig};

#[test]
fn test_fault_matrix_checksum_corruption() {
    let codec = FrameCodec::new(CodecConfig {
        max_frame_size: 1472,
        validate_checksum: true,
    });

    let frame = OwnedFrame::new_data(1, 100, 1000, 10, b"CORRUPTION_TEST_PAYLOAD".to_vec());
    let mut buf = vec![0u8; 1500];
    let len = codec.encode(&frame, &mut buf).expect("Encode");

    // Corrupt one bit in the payload
    buf[len - 1] ^= 0x01;

    let mut decode_buf = buf[..len].to_vec();
    let res = codec.decode(&mut decode_buf);
    assert!(res.is_err());
    assert_eq!(res.unwrap_err().code(), Some(ErrorCode::ChecksumFailed));
}

#[test]
fn test_fault_matrix_mtu_exceeded() {
    let codec = FrameCodec::new(CodecConfig {
        max_frame_size: 1472,
        validate_checksum: true,
    });

    // Create a payload that makes frame > 1472
    let huge_payload = vec![0x42; 1500];
    let frame = OwnedFrame::new_data(1, 1, 1000, 10, huge_payload);
    let mut buf = vec![0u8; 2000];
    let res = codec.encode(&frame, &mut buf);
    assert!(res.is_err());
    assert_eq!(res.unwrap_err().code(), Some(ErrorCode::FrameTooLarge));
}

#[test]
fn test_fault_matrix_reordering() {
    let sub_addr: SocketAddr = "127.0.0.1:19891".parse().unwrap();
    let pub_addr: SocketAddr = "127.0.0.1:19892".parse().unwrap();

    let sub = UdpSubscriber::new(UdpSubscriberConfig {
        bind_addr: sub_addr,
        publisher_addr: pub_addr,
        stream_id: 30,
        mode: DeliveryMode::ReliableOrdered,
        recovery_config: RecoveryConfig {
            retry_timeout: Duration::from_millis(20),
            max_retries: 3,
            max_replay_range: 100,
            max_outstanding_requests: 10,
        },
        max_frame_size: 1472,
        channel_capacity: 100,
    }).expect("Init subscriber");

    let mut publ = UdpPublisher::new(UdpPublisherConfig {
        bind_addr: pub_addr,
        target_addr: sub_addr,
        stream_id: 30,
        publisher_id: 3001,
        ring_buffer_capacity: 1000,
        max_frame_size: 1472,
        send_buffer_bytes: None,
    }).expect("Init publisher");

    let f1 = OwnedFrame::new_data(30, 1, 1000, 3001, b"P1".to_vec());
    let f2 = OwnedFrame::new_data(30, 2, 1001, 3001, b"P2".to_vec());
    let f3 = OwnedFrame::new_data(30, 3, 1002, 3001, b"P3".to_vec());

    // Send in reverse order: 3, then 2, then 1
    // 3 arrives: gap detected [1..2], buffered
    publ.publish_frame(&f3, true).expect("Send f3");
    // 2 arrives: gap still exists [1..1], 2 buffered
    publ.publish_frame(&f2, true).expect("Send f2");
    // 1 arrives: gap resolved! 1 delivered, then 2, then 3!
    publ.publish_frame(&f1, true).expect("Send f1");

    // All 3 delivered in strict sequential order 1, 2, 3
    let r1 = sub.recv_timeout(Duration::from_millis(500)).expect("r1");
    assert_eq!(r1.header.sequence, 1);
    let r2 = sub.recv_timeout(Duration::from_millis(500)).expect("r2");
    assert_eq!(r2.header.sequence, 2);
    let r3 = sub.recv_timeout(Duration::from_millis(500)).expect("r3");
    assert_eq!(r3.header.sequence, 3);
}

#[test]
fn test_fault_matrix_duplication_suppressed() {
    let sub_addr: SocketAddr = "127.0.0.1:19893".parse().unwrap();
    let pub_addr: SocketAddr = "127.0.0.1:19894".parse().unwrap();

    let sub = UdpSubscriber::new(UdpSubscriberConfig {
        bind_addr: sub_addr,
        publisher_addr: pub_addr,
        stream_id: 40,
        mode: DeliveryMode::ReliableOrdered,
        recovery_config: RecoveryConfig::default(),
        max_frame_size: 1472,
        channel_capacity: 100,
    }).expect("Init subscriber");

    let mut publ = UdpPublisher::new(UdpPublisherConfig {
        bind_addr: pub_addr,
        target_addr: sub_addr,
        stream_id: 40,
        publisher_id: 4001,
        ring_buffer_capacity: 1000,
        max_frame_size: 1472,
        send_buffer_bytes: None,
    }).expect("Init publisher");

    let f1 = OwnedFrame::new_data(40, 1, 1000, 4001, b"DUP_TEST".to_vec());

    // Send same frame 5 times
    for _ in 0..5 {
        publ.publish_frame(&f1, false).expect("Send f1");
    }

    // Only 1 delivery should happen
    let r1 = sub.recv_timeout(Duration::from_millis(300)).expect("First delivery");
    assert_eq!(r1.header.sequence, 1);

    // No further frames should arrive
    let r_none = sub.recv_timeout(Duration::from_millis(100));
    assert!(r_none.is_err());
}

#[test]
fn test_fault_matrix_publisher_restart() {
    let sub_addr: SocketAddr = "127.0.0.1:19895".parse().unwrap();
    let pub_addr: SocketAddr = "127.0.0.1:19896".parse().unwrap();

    let sub = UdpSubscriber::new(UdpSubscriberConfig {
        bind_addr: sub_addr,
        publisher_addr: pub_addr,
        stream_id: 50,
        mode: DeliveryMode::ReliableOrdered,
        recovery_config: RecoveryConfig::default(),
        max_frame_size: 1472,
        channel_capacity: 100,
    }).expect("Init subscriber");

    let mut publ = UdpPublisher::new(UdpPublisherConfig {
        bind_addr: pub_addr,
        target_addr: sub_addr,
        stream_id: 50,
        publisher_id: 5001,
        ring_buffer_capacity: 1000,
        max_frame_size: 1472,
        send_buffer_bytes: None,
    }).expect("Init publisher");

    // Publisher 5001 sends seq 1 and 2
    publ.publish(b"PUB1_SEQ1").unwrap();
    publ.publish(b"PUB1_SEQ2").unwrap();

    let r1 = sub.recv_timeout(Duration::from_millis(500)).expect("r1");
    assert_eq!(r1.header.publisher_id, 5001);
    assert_eq!(r1.header.sequence, 1);

    let r2 = sub.recv_timeout(Duration::from_millis(500)).expect("r2");
    assert_eq!(r2.header.publisher_id, 5001);
    assert_eq!(r2.header.sequence, 2);

    // Publisher restarts with new publisher_id 5002 and resets sequence back to 1
    publ.restart_with_new_publisher_id(5002);
    publ.publish(b"PUB2_SEQ1").unwrap();

    let r3 = sub.recv_timeout(Duration::from_millis(500)).expect("r3 after restart");
    assert_eq!(r3.header.publisher_id, 5002);
    assert_eq!(r3.header.sequence, 1);
    assert_eq!(r3.payload, b"PUB2_SEQ1");
}

#[test]
fn test_fault_matrix_retention_expiry_unavailable() {
    let mut engine = RecoveryEngine::new(RecoveryConfig::default());
    let _req = engine.create_replay_request(10, 20).expect("Create req");

    let unavail = ReplayUnavailablePayload {
        request_id: 1,
        first_unavailable: 10,
        last_unavailable: 15,
    };

    let res = engine.handle_replay_unavailable(&unavail);
    assert!(res.is_err());
    assert_eq!(engine.state(), RecoveryState::RecoveryFailed);
}
