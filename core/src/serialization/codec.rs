use byteorder::{BigEndian, ByteOrder};
use crc32c::crc32c;

use crate::error::{ErrorCode, RtdpError};
use crate::protocol::extensions::{is_known_extension, ExtensionIterator};
use crate::protocol::frame::{FrameView, OwnedFrame};
use crate::protocol::header::{
    FixedHeader, MessageType, DEFAULT_MAX_FRAME_SIZE, FIXED_HEADER_SIZE, MAGIC_RTDP,
    RTDP_VERSION_1,
};

pub struct CodecConfig {
    pub max_frame_size: usize,
    pub validate_checksum: bool,
}

impl Default for CodecConfig {
    fn default() -> Self {
        Self {
            max_frame_size: DEFAULT_MAX_FRAME_SIZE,
            validate_checksum: true,
        }
    }
}

pub struct FrameCodec {
    config: CodecConfig,
}

impl FrameCodec {
    pub fn new(config: CodecConfig) -> Self {
        Self { config }
    }

    pub fn with_default_config() -> Self {
        Self::new(CodecConfig::default())
    }

    /// Strict parser/validator matching Section 4.1 of the Handoff and Section 5 of RTDP-1 spec
    pub fn decode<'a>(&self, packet: &'a mut [u8]) -> Result<FrameView<'a>, RtdpError> {
        // Step 1: Read only the fixed 48-byte prefix first
        if packet.len() < FIXED_HEADER_SIZE {
            return Err(RtdpError::malformed(format!(
                "Frame length {} is less than minimum 48-byte header",
                packet.len()
            )));
        }

        // Check configured max frame size before heavy work
        if packet.len() > self.config.max_frame_size {
            return Err(RtdpError::frame_too_large(
                packet.len(),
                self.config.max_frame_size,
            ));
        }

        // Step 2: Validate magic and version
        let magic = BigEndian::read_u16(&packet[0..2]);
        if magic != MAGIC_RTDP {
            return Err(RtdpError::malformed(format!(
                "Invalid magic 0x{:04X}, expected 0x{:04X}",
                magic, MAGIC_RTDP
            )));
        }

        let version = packet[2];
        if version != RTDP_VERSION_1 {
            return Err(RtdpError::version_mismatch(format!(
                "Unsupported protocol version 0x{:02X}, expected 0x{:02X}",
                version, RTDP_VERSION_1
            )));
        }

        // Step 3: Validate header_length
        let header_length = BigEndian::read_u16(&packet[6..8]) as usize;
        if header_length < FIXED_HEADER_SIZE {
            return Err(RtdpError::malformed(format!(
                "header_length {} is below minimum 48",
                header_length
            )));
        }
        if header_length % 4 != 0 {
            return Err(RtdpError::malformed(format!(
                "header_length {} is not 4-byte aligned",
                header_length
            )));
        }
        if header_length > packet.len() {
            return Err(RtdpError::malformed(format!(
                "header_length {} exceeds total packet length {}",
                header_length,
                packet.len()
            )));
        }

        // Step 4: Validate payload_length against received frame size
        let payload_length = BigEndian::read_u32(&packet[40..44]) as usize;
        if header_length + payload_length != packet.len() {
            return Err(RtdpError::malformed(format!(
                "Frame size mismatch: header ({}) + payload ({}) != received packet ({})",
                header_length,
                payload_length,
                packet.len()
            )));
        }

        // Step 5: Validate TLV alignment and extension lengths
        if header_length > FIXED_HEADER_SIZE {
            let ext_slice = &packet[FIXED_HEADER_SIZE..header_length];
            let mut iter = ExtensionIterator::new(ext_slice);
            while let Some(res) = iter.next() {
                let ext = res?;
                if ext.is_critical && !is_known_extension(ext.ext_type) {
                    return Err(RtdpError::unsupported_extension(ext.ext_type));
                }
            }
        }

        // Step 6: Validate checksum before application decode
        let wire_checksum = BigEndian::read_u32(&packet[44..48]);
        if self.config.validate_checksum {
            // Zero out checksum field in packet buffer
            BigEndian::write_u32(&mut packet[44..48], 0);
            let calculated_checksum = crc32c(packet);
            // Restore wire checksum in buffer
            BigEndian::write_u32(&mut packet[44..48], wire_checksum);

            if calculated_checksum != wire_checksum {
                return Err(RtdpError::checksum_failed(
                    wire_checksum,
                    calculated_checksum,
                ));
            }
        }

        // Parse remaining header fields
        let header = FixedHeader::decode(&packet[..FIXED_HEADER_SIZE])?;

        // Message-specific payload structural invariants
        match header.message_type {
            MessageType::Heartbeat => {
                if payload_length != 0 {
                    return Err(RtdpError::malformed(format!(
                        "HEARTBEAT must have 0-length payload, got {}",
                        payload_length
                    )));
                }
            }
            MessageType::ReplayRequest => {
                if payload_length != 24 {
                    return Err(RtdpError::malformed(format!(
                        "REPLAY_REQUEST payload must be exactly 24 bytes, got {}",
                        payload_length
                    )));
                }
                let from_seq = BigEndian::read_u64(&packet[header_length + 8..header_length + 16]);
                let to_seq = BigEndian::read_u64(&packet[header_length + 16..header_length + 24]);
                if from_seq > to_seq {
                    return Err(RtdpError::Protocol {
                        code: ErrorCode::ReplayRangeInvalid,
                        detail: format!(
                            "REPLAY_REQUEST from_seq ({}) > to_seq ({})",
                            from_seq, to_seq
                        ),
                    });
                }
            }
            MessageType::ReplayUnavailable => {
                if payload_length != 24 {
                    return Err(RtdpError::malformed(format!(
                        "REPLAY_UNAVAILABLE payload must be exactly 24 bytes, got {}",
                        payload_length
                    )));
                }
            }
            MessageType::ReplayResponse => {
                if payload_length < 8 {
                    return Err(RtdpError::malformed(format!(
                        "REPLAY_RESPONSE payload must be at least 8 bytes (request_id), got {}",
                        payload_length
                    )));
                }
            }
            _ => {}
        }

        let extension_bytes = &packet[FIXED_HEADER_SIZE..header_length];
        let payload = &packet[header_length..];

        Ok(FrameView {
            header,
            extension_bytes,
            payload,
        })
    }

    /// Encode frame into destination buffer, calculating and populating CRC-32C
    pub fn encode(&self, frame: &OwnedFrame, dst: &mut [u8]) -> Result<usize, RtdpError> {
        let ext_len = frame.extension_bytes.len();
        let total_hl = FIXED_HEADER_SIZE + ext_len;
        if total_hl % 4 != 0 {
            return Err(RtdpError::malformed(
                "Extensions length makes header unaligned to 4 bytes",
            ));
        }

        let payload_len = frame.payload.len();
        let total_frame_len = total_hl + payload_len;

        if total_frame_len > self.config.max_frame_size {
            return Err(RtdpError::frame_too_large(
                total_frame_len,
                self.config.max_frame_size,
            ));
        }

        if dst.len() < total_frame_len {
            return Err(RtdpError::malformed(format!(
                "Destination buffer size {} < required {}",
                dst.len(),
                total_frame_len
            )));
        }

        let mut header = frame.header.clone();
        header.header_length = total_hl as u16;
        header.payload_length = payload_len as u32;
        header.checksum = 0; // Checksum field zeroed for calculation

        header.encode(&mut dst[..FIXED_HEADER_SIZE])?;

        if ext_len > 0 {
            dst[FIXED_HEADER_SIZE..total_hl].copy_from_slice(&frame.extension_bytes);
        }

        dst[total_hl..total_frame_len].copy_from_slice(&frame.payload);

        // Compute CRC-32C with checksum field zeroed
        let calculated_crc = crc32c(&dst[..total_frame_len]);
        BigEndian::write_u32(&mut dst[44..48], calculated_crc);

        Ok(total_frame_len)
    }
}
