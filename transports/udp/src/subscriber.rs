use std::net::{SocketAddr, UdpSocket};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::Duration;

use byteorder::{BigEndian, ByteOrder};
use crossbeam_channel::{bounded, Receiver};
use rtdp_core::delivery::{DeliveryMode, StreamReceiver};
use rtdp_core::error::RtdpError;
use rtdp_core::protocol::frame::OwnedFrame;
use rtdp_core::protocol::header::{FixedHeader, MessageType, FIXED_HEADER_SIZE};
use rtdp_core::recovery::{RecoveryConfig, ReplayRequestPayload, ReplayUnavailablePayload};
use rtdp_core::serialization::{CodecConfig, FrameCodec};

pub struct UdpSubscriberConfig {
    pub bind_addr: SocketAddr,
    pub publisher_addr: SocketAddr,
    pub stream_id: u64,
    pub mode: DeliveryMode,
    pub recovery_config: RecoveryConfig,
    pub max_frame_size: usize,
    pub channel_capacity: usize,
}

impl Default for UdpSubscriberConfig {
    fn default() -> Self {
        Self {
            bind_addr: "0.0.0.0:9876".parse().unwrap(),
            publisher_addr: "127.0.0.1:9875".parse().unwrap(),
            stream_id: 1,
            mode: DeliveryMode::ReliableOrdered,
            recovery_config: RecoveryConfig::default(),
            max_frame_size: 1472,
            channel_capacity: 10_000,
        }
    }
}

pub struct UdpSubscriber {
    socket: Arc<UdpSocket>,
    config: UdpSubscriberConfig,
    running: Arc<AtomicBool>,
    delivered_rx: Receiver<OwnedFrame>,
    worker_handle: Option<JoinHandle<()>>,
}

impl UdpSubscriber {
    pub fn new(config: UdpSubscriberConfig) -> Result<Self, RtdpError> {
        let socket = UdpSocket::bind(config.bind_addr).map_err(|e| RtdpError::Io(e.to_string()))?;
        socket
            .set_read_timeout(Some(Duration::from_millis(10)))
            .map_err(|e| RtdpError::Io(e.to_string()))?;

        let (delivered_tx, delivered_rx) = bounded(config.channel_capacity);
        let socket_arc = Arc::new(socket);
        let running = Arc::new(AtomicBool::new(true));

        let sock_clone = Arc::clone(&socket_arc);
        let run_clone = Arc::clone(&running);
        let stream_id = config.stream_id;
        let mode = config.mode;
        let recovery_conf = config.recovery_config.clone();
        let pub_addr = config.publisher_addr;
        let max_frame = config.max_frame_size;

        let worker_handle = thread::spawn(move || {
            let mut stream_rx = StreamReceiver::new(stream_id, mode, recovery_conf);
            let codec = FrameCodec::new(CodecConfig {
                max_frame_size: max_frame,
                validate_checksum: true,
            });
            let mut buf = vec![0u8; max_frame];

            while run_clone.load(Ordering::Relaxed) {
                // 1. Check recovery timeouts
                if let Ok(retries) = stream_rx.recovery.check_timeouts() {
                    for retry in retries {
                        send_replay_request(&sock_clone, &codec, pub_addr, stream_id, retry);
                    }
                }

                // 2. Read socket
                match sock_clone.recv_from(&mut buf) {
                    Ok((len, peer_addr)) => {
                        let mut pkt = &mut buf[..len];
                        if let Ok(view) = codec.decode(&mut pkt) {
                            if view.header.stream_id != stream_id && view.header.message_type != MessageType::ReplayUnavailable {
                                continue;
                            }

                            match view.header.message_type {
                                MessageType::Data => {
                                    let frame = OwnedFrame {
                                        header: view.header.clone(),
                                        extension_bytes: view.extension_bytes.to_vec(),
                                        payload: view.payload.to_vec(),
                                    };

                                    match stream_rx.process_data_frame(frame) {
                                        Ok((to_deliver, maybe_replay)) => {
                                            for f in to_deliver {
                                                let _ = delivered_tx.try_send(f);
                                            }
                                            if let Some(req) = maybe_replay {
                                                send_replay_request(&sock_clone, &codec, peer_addr, stream_id, req);
                                            }
                                        }
                                        Err(_) => {}
                                    }
                                }

                                MessageType::ReplayResponse => {
                                    if view.payload.len() >= 8 {
                                        let request_id = BigEndian::read_u64(&view.payload[0..8]);
                                        let original_payload = view.payload[8..].to_vec();

                                        let recovered_frame = OwnedFrame {
                                            header: FixedHeader {
                                                message_type: MessageType::Data,
                                                ..view.header.clone()
                                            },
                                            extension_bytes: view.extension_bytes.to_vec(),
                                            payload: original_payload,
                                        };

                                        if let Ok(Some(drained)) = stream_rx
                                            .recovery
                                            .handle_replay_response(request_id, recovered_frame)
                                        {
                                            for f in drained {
                                                let seq = f.header.sequence;
                                                stream_rx.highest_delivered_sequence =
                                                    stream_rx.highest_delivered_sequence.max(seq);
                                                let _ = delivered_tx.try_send(f);
                                            }
                                        }
                                    }
                                }

                                MessageType::ReplayUnavailable => {
                                    if let Ok(unavail) = ReplayUnavailablePayload::decode(view.payload) {
                                        let _ = stream_rx.recovery.handle_replay_unavailable(&unavail);
                                    }
                                }

                                _ => {}
                            }
                        }
                    }
                    Err(ref e)
                        if e.kind() == std::io::ErrorKind::WouldBlock
                            || e.kind() == std::io::ErrorKind::TimedOut =>
                    {
                        // Timeout tick
                    }
                    Err(_) => {
                        break;
                    }
                }
            }
        });

        Ok(Self {
            socket: socket_arc,
            config,
            running,
            delivered_rx,
            worker_handle: Some(worker_handle),
        })
    }

    pub fn local_addr(&self) -> SocketAddr {
        self.socket.local_addr().unwrap()
    }

    pub fn config(&self) -> &UdpSubscriberConfig {
        &self.config
    }

    /// Receive next in-order delivered frame
    pub fn recv_timeout(&self, timeout: Duration) -> Result<OwnedFrame, crossbeam_channel::RecvTimeoutError> {
        self.delivered_rx.recv_timeout(timeout)
    }

    /// Try receiving next delivered frame non-blocking
    pub fn try_recv(&self) -> Result<OwnedFrame, crossbeam_channel::TryRecvError> {
        self.delivered_rx.try_recv()
    }
}

impl Drop for UdpSubscriber {
    fn drop(&mut self) {
        self.running.store(false, Ordering::Relaxed);
        if let Some(th) = self.worker_handle.take() {
            let _ = th.join();
        }
    }
}

fn send_replay_request(
    socket: &UdpSocket,
    codec: &FrameCodec,
    target: SocketAddr,
    stream_id: u64,
    req: ReplayRequestPayload,
) {
    let mut payload = [0u8; 24];
    let _ = req.encode(&mut payload);

    let header = FixedHeader {
        magic: rtdp_core::protocol::MAGIC_RTDP,
        version: rtdp_core::protocol::RTDP_VERSION_1,
        flags: rtdp_core::protocol::FrameFlags(0),
        message_type: MessageType::ReplayRequest,
        header_length: FIXED_HEADER_SIZE as u16,
        stream_id,
        sequence: 0,
        timestamp_ns: 0,
        publisher_id: 0,
        payload_length: 24,
        checksum: 0,
    };

    let frame = OwnedFrame {
        header,
        extension_bytes: Vec::new(),
        payload: payload.to_vec(),
    };

    let mut buf = vec![0u8; 1500];
    if let Ok(len) = codec.encode(&frame, &mut buf) {
        let _ = socket.send_to(&buf[..len], target);
    }
}
