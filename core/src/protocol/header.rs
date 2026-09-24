use byteorder::{BigEndian, ByteOrder};
use crate::error::{ErrorCode, RtdpError};

pub const MAGIC_RTDP: u16 = 0x5254; // "RT"
pub const RTDP_VERSION_1: u8 = 0x01;
pub const FIXED_HEADER_SIZE: usize = 48;
pub const DEFAULT_MAX_FRAME_SIZE: usize = 1472; // MTU safe for Ethernet UDP

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u16)]
pub enum MessageType {
    Data = 0x0001,
    Heartbeat = 0x0002,
    ReplayRequest = 0x0003,
    ReplayResponse = 0x0004,
    ReplayUnavailable = 0x0005,
    SnapshotRequest = 0x0006,
    SnapshotResponse = 0x0007,
    Error = 0x0008,
    Experimental(u16),
}

impl MessageType {
    pub fn from_u16(val: u16) -> Result<Self, RtdpError> {
        match val {
            0x0001 => Ok(Self::Data),
            0x0002 => Ok(Self::Heartbeat),
            0x0003 => Ok(Self::ReplayRequest),
            0x0004 => Ok(Self::ReplayResponse),
            0x0005 => Ok(Self::ReplayUnavailable),
            0x0006 => Ok(Self::SnapshotRequest),
            0x0007 => Ok(Self::SnapshotResponse),
            0x0008 => Ok(Self::Error),
            0xFF00..=0xFFFF => Ok(Self::Experimental(val)),
            _ => Err(RtdpError::Protocol {
                code: ErrorCode::UnsupportedMessageType,
                detail: format!("Unknown message type: 0x{:04X}", val),
            }),
        }
    }

    pub fn as_u16(&self) -> u16 {
        match self {
            Self::Data => 0x0001,
            Self::Heartbeat => 0x0002,
            Self::ReplayRequest => 0x0003,
            Self::ReplayResponse => 0x0004,
            Self::ReplayUnavailable => 0x0005,
            Self::SnapshotRequest => 0x0006,
            Self::SnapshotResponse => 0x0007,
            Self::Error => 0x0008,
            Self::Experimental(v) => *v,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct FrameFlags(pub u8);

impl FrameFlags {
    pub const FRAGMENTED: u8 = 1 << 0;
    pub const LAST_FRAGMENT: u8 = 1 << 1;
    pub const COMPRESSED: u8 = 1 << 2;

    pub fn is_fragmented(&self) -> bool {
        (self.0 & Self::FRAGMENTED) != 0
    }

    pub fn is_last_fragment(&self) -> bool {
        (self.0 & Self::LAST_FRAGMENT) != 0
    }

    pub fn is_compressed(&self) -> bool {
        (self.0 & Self::COMPRESSED) != 0
    }
}

/// Normative 48-byte Fixed Header layout
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FixedHeader {
    pub magic: u16,
    pub version: u8,
    pub flags: FrameFlags,
    pub message_type: MessageType,
    pub header_length: u16,
    pub stream_id: u64,
    pub sequence: u64,
    pub timestamp_ns: u64,
    pub publisher_id: u64,
    pub payload_length: u32,
    pub checksum: u32,
}

impl FixedHeader {
    pub fn new_data(
        stream_id: u64,
        sequence: u64,
        timestamp_ns: u64,
        publisher_id: u64,
        payload_length: u32,
        header_length: u16,
    ) -> Self {
        Self {
            magic: MAGIC_RTDP,
            version: RTDP_VERSION_1,
            flags: FrameFlags(0),
            message_type: MessageType::Data,
            header_length,
            stream_id,
            sequence,
            timestamp_ns,
            publisher_id,
            payload_length,
            checksum: 0,
        }
    }

    pub fn encode(&self, dst: &mut [u8]) -> Result<(), RtdpError> {
        if dst.len() < FIXED_HEADER_SIZE {
            return Err(RtdpError::malformed("Destination buffer too short for fixed header"));
        }

        BigEndian::write_u16(&mut dst[0..2], self.magic);
        dst[2] = self.version;
        dst[3] = self.flags.0;
        BigEndian::write_u16(&mut dst[4..6], self.message_type.as_u16());
        BigEndian::write_u16(&mut dst[6..8], self.header_length);
        BigEndian::write_u64(&mut dst[8..16], self.stream_id);
        BigEndian::write_u64(&mut dst[16..24], self.sequence);
        BigEndian::write_u64(&mut dst[24..32], self.timestamp_ns);
        BigEndian::write_u64(&mut dst[32..40], self.publisher_id);
        BigEndian::write_u32(&mut dst[40..44], self.payload_length);
        BigEndian::write_u32(&mut dst[44..48], self.checksum);

        Ok(())
    }

    pub fn decode(src: &[u8]) -> Result<Self, RtdpError> {
        if src.len() < FIXED_HEADER_SIZE {
            return Err(RtdpError::malformed(format!(
                "Packet length {} is less than 48-byte fixed header",
                src.len()
            )));
        }

        let magic = BigEndian::read_u16(&src[0..2]);
        let version = src[2];
        let flags = FrameFlags(src[3]);
        let raw_msg_type = BigEndian::read_u16(&src[4..6]);
        let header_length = BigEndian::read_u16(&src[6..8]);
        let stream_id = BigEndian::read_u64(&src[8..16]);
        let sequence = BigEndian::read_u64(&src[16..24]);
        let timestamp_ns = BigEndian::read_u64(&src[24..32]);
        let publisher_id = BigEndian::read_u64(&src[32..40]);
        let payload_length = BigEndian::read_u32(&src[40..44]);
        let checksum = BigEndian::read_u32(&src[44..48]);

        // Step 2: Validate magic
        if magic != MAGIC_RTDP {
            return Err(RtdpError::malformed(format!(
                "Invalid magic 0x{:04X}, expected 0x{:04X}",
                magic, MAGIC_RTDP
            )));
        }

        // Step 3: Validate version
        if version != RTDP_VERSION_1 {
            return Err(RtdpError::version_mismatch(format!(
                "Unsupported protocol version 0x{:02X}, expected 0x{:02X}",
                version, RTDP_VERSION_1
            )));
        }

        let message_type = MessageType::from_u16(raw_msg_type)?;

        Ok(Self {
            magic,
            version,
            flags,
            message_type,
            header_length,
            stream_id,
            sequence,
            timestamp_ns,
            publisher_id,
            payload_length,
            checksum,
        })
    }
}
