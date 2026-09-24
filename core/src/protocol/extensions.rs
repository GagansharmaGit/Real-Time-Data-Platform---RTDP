use byteorder::{BigEndian, ByteOrder};
use crate::error::RtdpError;

pub const CRITICAL_EXTENSION_BIT: u16 = 0x8000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeaderExtension<'a> {
    pub ext_type: u16,
    pub is_critical: bool,
    pub value: &'a [u8],
}

impl<'a> HeaderExtension<'a> {
    pub fn new(ext_type: u16, value: &'a [u8]) -> Self {
        let is_critical = (ext_type & CRITICAL_EXTENSION_BIT) != 0;
        Self {
            ext_type,
            is_critical,
            value,
        }
    }

    /// Padded byte length including 4-byte TLV header
    pub fn wire_len(&self) -> usize {
        let val_len = self.value.len();
        let padded_val_len = (val_len + 3) & !3;
        4 + padded_val_len
    }

    pub fn encode(&self, dst: &mut [u8]) -> Result<usize, RtdpError> {
        let wire_len = self.wire_len();
        if dst.len() < wire_len {
            return Err(RtdpError::malformed("Buffer too short for extension TLV"));
        }

        BigEndian::write_u16(&mut dst[0..2], self.ext_type);
        BigEndian::write_u16(&mut dst[2..4], self.value.len() as u16);
        dst[4..4 + self.value.len()].copy_from_slice(self.value);

        // Zero out padding bytes
        for b in &mut dst[4 + self.value.len()..wire_len] {
            *b = 0;
        }

        Ok(wire_len)
    }
}

pub struct ExtensionIterator<'a> {
    data: &'a [u8],
    offset: usize,
}

impl<'a> ExtensionIterator<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Self { data, offset: 0 }
    }
}

impl<'a> Iterator for ExtensionIterator<'a> {
    type Item = Result<HeaderExtension<'a>, RtdpError>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.offset >= self.data.len() {
            return None;
        }

        let remaining = &self.data[self.offset..];
        if remaining.len() < 4 {
            return Some(Err(RtdpError::malformed(
                "Truncated extension TLV header (less than 4 bytes remaining)",
            )));
        }

        let ext_type = BigEndian::read_u16(&remaining[0..2]);
        let val_len = BigEndian::read_u16(&remaining[2..4]) as usize;
        let padded_val_len = (val_len + 3) & !3;
        let total_tlv_len = 4 + padded_val_len;

        if remaining.len() < total_tlv_len {
            return Some(Err(RtdpError::malformed(format!(
                "Extension TLV requires {} bytes, but only {} remaining",
                total_tlv_len,
                remaining.len()
            ))));
        }

        let is_critical = (ext_type & CRITICAL_EXTENSION_BIT) != 0;
        let value = &remaining[4..4 + val_len];

        self.offset += total_tlv_len;

        Some(Ok(HeaderExtension {
            ext_type,
            is_critical,
            value,
        }))
    }
}

/// Known standard extensions
pub mod standard_types {
    pub const SOURCE_INGRESS_TS: u16 = 0x0001; // Non-critical, 8 bytes
    pub const CORRELATION_ID: u16 = 0x0002;    // Non-critical, 16 bytes
    pub const REQUIRED_CIPHER_SUITE: u16 = 0x8001; // Critical
}

pub fn is_known_extension(ext_type: u16) -> bool {
    matches!(
        ext_type,
        standard_types::SOURCE_INGRESS_TS | standard_types::CORRELATION_ID
    )
}
