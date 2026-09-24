use std::net::{SocketAddr, UdpSocket};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use byteorder::{BigEndian, ByteOrder};
use parking_lot::Mutex;
use rtdp_core::buffers::BoundedRingBuffer;
use rtdp_core::error::RtdpError;
use rtdp_core::protocol::frame::OwnedFrame;
use rtdp_core::protocol::header::{FixedHeader, MessageType, FIXED_HEADER_SIZE};
use rtdp_core::recovery::{ReplayRequestPayload, ReplayUnavailablePayload};
use rtdp_core::serialization::{CodecConfig, FrameCodec};

pub struct UdpPublisherConfig {
    pub bind_addr: SocketAddr,
    pub target_addr: SocketAddr,
    pub stream_id: u64,
    pub publisher_id: u64,
    pub ring_buffer_capacity: usize,
    pub max_frame_size: usize,
    pub send_buffer_bytes: Option<usize>,
}

impl Default for UdpPublisherConfig {
    fn default() -> Self {
        Self {
            bind_addr: "0.0.0.0:0".parse().unwrap(),
            target_addr: "127.0.0.1:9876".parse().unwrap(),
            stream_id: 1,
            publisher_id: 100,
            ring_buffer_capacity: 10_000,
            max_frame_size: 1472,
            send_buffer_bytes: Some(1024 * 1024), // 1MB
        }
    }
}

pub struct UdpPublisher {
    socket: Arc<UdpSocket>,
    config: UdpPublisherConfig,
    codec: FrameCodec,
    ring_buffer: Arc<Mutex<BoundedRingBuffer>>,
    sequence_counter: u64,
    running: Arc<AtomicBool>,
    replay_thread: Option<JoinHandle<()>>,
}

impl UdpPublisher {
    pub fn new(config: UdpPublisherConfig) -> Result<Self, RtdpError> {
        let socket = UdpSocket::bind(config.bind_addr).map_err(|e| RtdpError::Io(e.to_string()))?;
        socket
            .set_read_timeout(Some(Duration::from_millis(50)))
            .map_err(|e| RtdpError::Io(e.to_string()))?;

        let ring_buffer = Arc::new(Mutex::new(BoundedRingBuffer::new(config.ring_buffer_capacity)));
        let codec = FrameCodec::new(CodecConfig {
            max_frame_size: config.max_frame_size,
            validate_checksum: true,
        });

        let socket_arc = Arc::new(socket);
        let running = Arc::new(AtomicBool::new(true));

        // Start background replay listener thread to answer REPLAY_REQUESTs
        let sock_clone = Arc::clone(&socket_arc);
        let ring_clone = Arc::clone(&ring_buffer);
        let run_clone = Arc::clone(&running);
        let max_frame = config.max_frame_size;
        let stream_id = config.stream_id;
        let publisher_id = config.publisher_id;

        let replay_thread = thread::spawn(move || {
            let mut buf = vec![0u8; max_frame];
            let replay_codec = FrameCodec::new(CodecConfig {
                max_frame_size: max_frame,
                validate_checksum: true,
            });

            while run_clone.load(Ordering::Relaxed) {
                match sock_clone.recv_from(&mut buf) {
                    Ok((len, peer_addr)) => {
                        let mut pkt = &mut buf[..len];
                        if let Ok(view) = replay_codec.decode(&mut pkt) {
                            if view.header.message_type == MessageType::ReplayRequest {
                                if let Ok(req) = ReplayRequestPayload::decode(view.payload) {
                                    handle_replay_request(
                                        &sock_clone,
                                        &replay_codec,
                                        &ring_clone,
                                        peer_addr,
                                        stream_id,
                                        publisher_id,
                                        req,
                                    );
                                }
                            }
                        }
                    }
                    Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock || e.kind() == std::io::ErrorKind::TimedOut => {
                        // Timeout loop check
                    }
                    Err(_) => {
                        // socket closed or error
                        break;
                    }
                }
            }
        });

        Ok(Self {
            socket: socket_arc,
            config,
            codec,
            ring_buffer,
            sequence_counter: 1, // Sequences start at 1
            running,
            replay_thread: Some(replay_thread),
        })
    }

    pub fn local_addr(&self) -> SocketAddr {
        self.socket.local_addr().unwrap()
    }

    /// Publish application payload to target address
    pub fn publish(&mut self, payload: &[u8]) -> Result<u64, RtdpError> {
        let seq = self.sequence_counter;
        self.sequence_counter += 1;

        let now_ns = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos() as u64;

        let frame = OwnedFrame::new_data(
            self.config.stream_id,
            seq,
            now_ns,
            self.config.publisher_id,
            payload.to_vec(),
        );

        // Store in bounded ring buffer for replay
        self.ring_buffer.lock().push(frame.clone());

        // Encode and send
        let mut encode_buf = vec![0u8; self.config.max_frame_size];
        let encoded_len = self.codec.encode(&frame, &mut encode_buf)?;

        self.socket
            .send_to(&encode_buf[..encoded_len], self.config.target_addr)
            .map_err(|e| RtdpError::Io(e.to_string()))?;

        Ok(seq)
    }

    /// Publish raw frame with explicit fault injection (e.g. skip sequence or corrupt)
    pub fn publish_frame(&mut self, frame: &OwnedFrame, store_in_ring: bool) -> Result<usize, RtdpError> {
        if store_in_ring {
            self.ring_buffer.lock().push(frame.clone());
        }

        let mut encode_buf = vec![0u8; self.config.max_frame_size];
        let encoded_len = self.codec.encode(frame, &mut encode_buf)?;

        self.socket
            .send_to(&encode_buf[..encoded_len], self.config.target_addr)
            .map_err(|e| RtdpError::Io(e.to_string()))?;

        Ok(encoded_len)
    }

    /// Store a frame in the ring buffer without transmitting on wire (simulating lost packet)
    pub fn store_in_ring_only(&mut self, frame: OwnedFrame) {
        self.ring_buffer.lock().push(frame);
    }

    /// Reset publisher identity (e.g., restart scenario)
    pub fn restart_with_new_publisher_id(&mut self, new_publisher_id: u64) {
        self.config.publisher_id = new_publisher_id;
        self.sequence_counter = 1;
        self.ring_buffer.lock().clear();
    }
}

impl Drop for UdpPublisher {
    fn drop(&mut self) {
        self.running.store(false, Ordering::Relaxed);
        if let Some(th) = self.replay_thread.take() {
            let _ = th.join();
        }
    }
}

fn handle_replay_request(
    socket: &UdpSocket,
    codec: &FrameCodec,
    ring: &Arc<Mutex<BoundedRingBuffer>>,
    peer: SocketAddr,
    stream_id: u64,
    publisher_id: u64,
    req: ReplayRequestPayload,
) {
    let frames_res = ring.lock().get_range(req.from_sequence, req.to_sequence);
    match frames_res {
        Ok(frames) => {
            for original_frame in frames {
                // Construct REPLAY_RESPONSE
                // Payload begins with request_id (8 bytes) + original payload
                let mut resp_payload = Vec::with_capacity(8 + original_frame.payload.len());
                let mut req_id_bytes = [0u8; 8];
                BigEndian::write_u64(&mut req_id_bytes, req.request_id);
                resp_payload.extend_from_slice(&req_id_bytes);
                resp_payload.extend_from_slice(&original_frame.payload);

                let header = FixedHeader {
                    magic: rtdp_core::protocol::MAGIC_RTDP,
                    version: rtdp_core::protocol::RTDP_VERSION_1,
                    flags: rtdp_core::protocol::FrameFlags(0),
                    message_type: MessageType::ReplayResponse,
                    header_length: FIXED_HEADER_SIZE as u16,
                    stream_id: original_frame.header.stream_id,
                    sequence: original_frame.header.sequence,
                    timestamp_ns: original_frame.header.timestamp_ns,
                    publisher_id: original_frame.header.publisher_id,
                    payload_length: resp_payload.len() as u32,
                    checksum: 0,
                };

                let resp_frame = OwnedFrame {
                    header,
                    extension_bytes: Vec::new(),
                    payload: resp_payload,
                };

                let mut send_buf = vec![0u8; 1500];
                if let Ok(len) = codec.encode(&resp_frame, &mut send_buf) {
                    let _ = socket.send_to(&send_buf[..len], peer);
                }
            }
        }
        Err((_, first_unavail, last_unavail)) => {
            // Send REPLAY_UNAVAILABLE
            let mut unavail_payload = [0u8; 24];
            let payload = ReplayUnavailablePayload {
                request_id: req.request_id,
                first_unavailable: first_unavail,
                last_unavailable: last_unavail,
            };
            let _ = payload.encode(&mut unavail_payload);

            let header = FixedHeader {
                magic: rtdp_core::protocol::MAGIC_RTDP,
                version: rtdp_core::protocol::RTDP_VERSION_1,
                flags: rtdp_core::protocol::FrameFlags(0),
                message_type: MessageType::ReplayUnavailable,
                header_length: FIXED_HEADER_SIZE as u16,
                stream_id,
                sequence: 0,
                timestamp_ns: 0,
                publisher_id,
                payload_length: 24,
                checksum: 0,
            };

            let unavail_frame = OwnedFrame {
                header,
                extension_bytes: Vec::new(),
                payload: unavail_payload.to_vec(),
            };

            let mut send_buf = vec![0u8; 1500];
            if let Ok(len) = codec.encode(&unavail_frame, &mut send_buf) {
                let _ = socket.send_to(&send_buf[..len], peer);
            }
        }
    }
}
