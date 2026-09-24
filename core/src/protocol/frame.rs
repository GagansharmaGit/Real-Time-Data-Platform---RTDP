use crate::protocol::extensions::ExtensionIterator;
use crate::protocol::header::FixedHeader;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrameView<'a> {
    pub header: FixedHeader,
    pub extension_bytes: &'a [u8],
    pub payload: &'a [u8],
}

impl<'a> FrameView<'a> {
    pub fn extensions(&self) -> ExtensionIterator<'a> {
        ExtensionIterator::new(self.extension_bytes)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OwnedFrame {
    pub header: FixedHeader,
    pub extension_bytes: Vec<u8>,
    pub payload: Vec<u8>,
}

impl OwnedFrame {
    pub fn new_data(
        stream_id: u64,
        sequence: u64,
        timestamp_ns: u64,
        publisher_id: u64,
        payload: Vec<u8>,
    ) -> Self {
        let payload_len = payload.len() as u32;
        let header = FixedHeader::new_data(
            stream_id,
            sequence,
            timestamp_ns,
            publisher_id,
            payload_len,
            48,
        );
        Self {
            header,
            extension_bytes: Vec::new(),
            payload,
        }
    }

    pub fn view(&self) -> FrameView<'_> {
        FrameView {
            header: self.header.clone(),
            extension_bytes: &self.extension_bytes,
            payload: &self.payload,
        }
    }
}
