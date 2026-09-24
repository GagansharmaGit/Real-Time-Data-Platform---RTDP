use std::fmt;
use thiserror::Error;

/// RTDP/1 Normative Protocol Error Codes (Appendix A / error-codes.md)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u16)]
pub enum ErrorCode {
    VersionMismatch = 0x0001,
    MalformedFrame = 0x0002,
    ChecksumFailed = 0x0003,
    UnsupportedMessageType = 0x0004,
    UnsupportedExtension = 0x0005,
    ReplayUnavailable = 0x0006,
    ReplayRangeInvalid = 0x0007,
    StreamNotFound = 0x0008,
    PublisherNotAuthorized = 0x0009,
    FrameTooLarge = 0x000A,
    RateLimited = 0x000B,
    Backpressure = 0x000C,
}

impl ErrorCode {
    pub fn from_u16(val: u16) -> Option<Self> {
        match val {
            0x0001 => Some(Self::VersionMismatch),
            0x0002 => Some(Self::MalformedFrame),
            0x0003 => Some(Self::ChecksumFailed),
            0x0004 => Some(Self::UnsupportedMessageType),
            0x0005 => Some(Self::UnsupportedExtension),
            0x0006 => Some(Self::ReplayUnavailable),
            0x0007 => Some(Self::ReplayRangeInvalid),
            0x0008 => Some(Self::StreamNotFound),
            0x0009 => Some(Self::PublisherNotAuthorized),
            0x000A => Some(Self::FrameTooLarge),
            0x000B => Some(Self::RateLimited),
            0x000C => Some(Self::Backpressure),
            _ => None,
        }
    }

    pub fn as_u16(&self) -> u16 {
        *self as u16
    }
}

impl fmt::Display for ErrorCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::VersionMismatch => write!(f, "VERSION_MISMATCH (0x0001)"),
            Self::MalformedFrame => write!(f, "MALFORMED_FRAME (0x0002)"),
            Self::ChecksumFailed => write!(f, "CHECKSUM_FAILED (0x0003)"),
            Self::UnsupportedMessageType => write!(f, "UNSUPPORTED_MESSAGE_TYPE (0x0004)"),
            Self::UnsupportedExtension => write!(f, "UNSUPPORTED_EXTENSION (0x0005)"),
            Self::ReplayUnavailable => write!(f, "REPLAY_UNAVAILABLE (0x0006)"),
            Self::ReplayRangeInvalid => write!(f, "REPLAY_RANGE_INVALID (0x0007)"),
            Self::StreamNotFound => write!(f, "STREAM_NOT_FOUND (0x0008)"),
            Self::PublisherNotAuthorized => write!(f, "PUBLISHER_NOT_AUTHORIZED (0x0009)"),
            Self::FrameTooLarge => write!(f, "FRAME_TOO_LARGE (0x000A)"),
            Self::RateLimited => write!(f, "RATE_LIMITED (0x000B)"),
            Self::Backpressure => write!(f, "BACKPRESSURE (0x000C)"),
        }
    }
}

#[derive(Error, Debug, Clone, PartialEq, Eq)]
pub enum RtdpError {
    #[error("Protocol error: {code} - {detail}")]
    Protocol {
        code: ErrorCode,
        detail: String,
    },

    #[error("IO error: {0}")]
    Io(String),

    #[error("Buffer overflow or exhausted capacity")]
    BufferExhausted,

    #[error("Stream gap unrecoverable: sequence {from} to {to}")]
    UnrecoverableGap {
        from: u64,
        to: u64,
    },
}

impl RtdpError {
    pub fn malformed(detail: impl Into<String>) -> Self {
        Self::Protocol {
            code: ErrorCode::MalformedFrame,
            detail: detail.into(),
        }
    }

    pub fn version_mismatch(detail: impl Into<String>) -> Self {
        Self::Protocol {
            code: ErrorCode::VersionMismatch,
            detail: detail.into(),
        }
    }

    pub fn checksum_failed(expected: u32, calculated: u32) -> Self {
        Self::Protocol {
            code: ErrorCode::ChecksumFailed,
            detail: format!("Wire checksum 0x{:08X} != calculated 0x{:08X}", expected, calculated),
        }
    }

    pub fn unsupported_extension(ext_type: u16) -> Self {
        Self::Protocol {
            code: ErrorCode::UnsupportedExtension,
            detail: format!("Critical extension type 0x{:04X} is not supported", ext_type),
        }
    }

    pub fn frame_too_large(size: usize, max: usize) -> Self {
        Self::Protocol {
            code: ErrorCode::FrameTooLarge,
            detail: format!("Frame length {} exceeds configured maximum {}", size, max),
        }
    }

    pub fn code(&self) -> Option<ErrorCode> {
        match self {
            Self::Protocol { code, .. } => Some(*code),
            _ => None,
        }
    }
}
