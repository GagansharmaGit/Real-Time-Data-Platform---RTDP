use std::time::Duration;
use rtdp_core::delivery::DeliveryMode;
use rtdp_core::protocol::frame::OwnedFrame;
use rtdp_core::recovery::RecoveryConfig;
use rtdp_transport_udp::{UdpPublisher, UdpPublisherConfig, UdpSubscriber, UdpSubscriberConfig};

#[test]
fn test_udp_direct_publish_and_receive() {
    let sub_addr: std::net::SocketAddr = "127.0.0.1:19871".parse().unwrap();
    let pub_addr: std::net::SocketAddr = "127.0.0.1:19872".parse().unwrap();

    let sub = UdpSubscriber::new(UdpSubscriberConfig {
        bind_addr: sub_addr,
        publisher_addr: pub_addr,
        stream_id: 10,
        mode: DeliveryMode::ReliableOrdered,
        recovery_config: RecoveryConfig::default(),
        max_frame_size: 1472,
        channel_capacity: 100,
    }).expect("Init subscriber");

    let mut publ = UdpPublisher::new(UdpPublisherConfig {
        bind_addr: pub_addr,
        target_addr: sub_addr,
        stream_id: 10,
        publisher_id: 1001,
        ring_buffer_capacity: 1000,
        max_frame_size: 1472,
        send_buffer_bytes: None,
    }).expect("Init publisher");

    // Publish 5 messages
    for i in 1..=5 {
        publ.publish(format!("msg-{}", i).as_bytes()).expect("Publish");
    }

    // Verify subscriber receives all 5 in order
    for i in 1..=5 {
        let frame = sub.recv_timeout(Duration::from_millis(500)).expect("Received frame");
        assert_eq!(frame.header.sequence, i);
        assert_eq!(frame.payload, format!("msg-{}", i).as_bytes());
    }
}

#[test]
fn test_udp_loss_gap_detection_and_selective_replay() {
    let sub_addr: std::net::SocketAddr = "127.0.0.1:19881".parse().unwrap();
    let pub_addr: std::net::SocketAddr = "127.0.0.1:19882".parse().unwrap();

    let sub = UdpSubscriber::new(UdpSubscriberConfig {
        bind_addr: sub_addr,
        publisher_addr: pub_addr,
        stream_id: 20,
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
        stream_id: 20,
        publisher_id: 2001,
        ring_buffer_capacity: 1000,
        max_frame_size: 1472,
        send_buffer_bytes: None,
    }).expect("Init publisher");

    // Construct 3 frames
    let f1 = OwnedFrame::new_data(20, 1, 1000, 2001, b"PACKET_1".to_vec());
    let f2 = OwnedFrame::new_data(20, 2, 1001, 2001, b"PACKET_2_DROPPED_IN_FLIGHT".to_vec());
    let f3 = OwnedFrame::new_data(20, 3, 1002, 2001, b"PACKET_3".to_vec());

    // 1. Send frame 1
    publ.publish_frame(&f1, true).expect("Send f1");

    // 2. Store frame 2 in ring buffer ONLY (simulating network loss)
    publ.store_in_ring_only(f2);

    // 3. Send frame 3
    publ.publish_frame(&f3, true).expect("Send f3");

    // The subscriber receives f1 first
    let r1 = sub.recv_timeout(Duration::from_millis(500)).expect("Received r1");
    assert_eq!(r1.header.sequence, 1);
    assert_eq!(r1.payload, b"PACKET_1");

    // Then subscriber receives f3, detects gap [2..2], sends REPLAY_REQUEST to pub_addr.
    // Publisher answers with REPLAY_RESPONSE for seq 2.
    // Subscriber receives replay, unblocks f2, then unblocks buffered f3!
    let r2 = sub.recv_timeout(Duration::from_millis(1000)).expect("Recovered r2 via replay");
    assert_eq!(r2.header.sequence, 2);
    assert_eq!(r2.payload, b"PACKET_2_DROPPED_IN_FLIGHT");

    let r3 = sub.recv_timeout(Duration::from_millis(500)).expect("Delivered buffered r3");
    assert_eq!(r3.header.sequence, 3);
    assert_eq!(r3.payload, b"PACKET_3");
}
