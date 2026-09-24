use std::slice;
use rtdp_core::protocol::frame::OwnedFrame;
use rtdp_core::protocol::header::{FixedHeader, FrameFlags, MessageType};
use rtdp_core::serialization::{CodecConfig, FrameCodec};

#[allow(non_camel_case_types)]
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct rtdp_header_t {
    pub magic: u16,
    pub version: u8,
    pub flags: u8,
    pub message_type: u16,
    pub header_length: u16,
    pub stream_id: u64,
    pub sequence: u64,
    pub timestamp_ns: u64,
    pub publisher_id: u64,
    pub payload_length: u32,
    pub checksum: u32,
}

#[allow(non_camel_case_types)]
pub struct rtdp_codec_t {
    inner: FrameCodec,
}

#[no_mangle]
pub extern "C" fn rtdp_codec_create(max_frame_size: usize, validate_checksum: bool) -> *mut rtdp_codec_t {
    let codec = FrameCodec::new(CodecConfig {
        max_frame_size,
        validate_checksum,
    });
    Box::into_raw(Box::new(rtdp_codec_t { inner: codec }))
}

#[no_mangle]
pub extern "C" fn rtdp_codec_destroy(codec: *mut rtdp_codec_t) {
    if !codec.is_null() {
        unsafe {
            drop(Box::from_raw(codec));
        }
    }
}

/// Decode packet buffer in-place.
/// Returns 0 on success, or negative error code matching ErrorCode (e.g. -1 for VERSION_MISMATCH, -2 for MALFORMED_FRAME, -3 for CHECKSUM_FAILED).
#[no_mangle]
pub extern "C" fn rtdp_decode(
    codec: *mut rtdp_codec_t,
    buf: *mut u8,
    len: usize,
    out_header: *mut rtdp_header_t,
    out_payload: *mut *const u8,
    out_payload_len: *mut usize,
) -> i32 {
    if codec.is_null() || buf.is_null() {
        return -0x0002; // MALFORMED_FRAME
    }

    let codec_ref = unsafe { &*codec };
    let slice = unsafe { slice::from_raw_parts_mut(buf, len) };

    match codec_ref.inner.decode(slice) {
        Ok(view) => {
            if !out_header.is_null() {
                unsafe {
                    *out_header = rtdp_header_t {
                        magic: view.header.magic,
                        version: view.header.version,
                        flags: view.header.flags.0,
                        message_type: view.header.message_type.as_u16(),
                        header_length: view.header.header_length,
                        stream_id: view.header.stream_id,
                        sequence: view.header.sequence,
                        timestamp_ns: view.header.timestamp_ns,
                        publisher_id: view.header.publisher_id,
                        payload_length: view.header.payload_length,
                        checksum: view.header.checksum,
                    };
                }
            }
            if !out_payload.is_null() {
                unsafe {
                    *out_payload = view.payload.as_ptr();
                }
            }
            if !out_payload_len.is_null() {
                unsafe {
                    *out_payload_len = view.payload.len();
                }
            }
            0
        }
        Err(e) => {
            if let Some(code) = e.code() {
                -(code.as_u16() as i32)
            } else {
                -0x0002
            }
        }
    }
}

/// Encode frame into out_buf.
/// Returns 0 on success, or negative error code on error.
#[no_mangle]
pub extern "C" fn rtdp_encode(
    codec: *mut rtdp_codec_t,
    in_header: *const rtdp_header_t,
    payload: *const u8,
    payload_len: usize,
    out_buf: *mut u8,
    out_cap: usize,
    out_len: *mut usize,
) -> i32 {
    if codec.is_null() || in_header.is_null() || out_buf.is_null() || out_len.is_null() {
        return -0x0002;
    }

    let codec_ref = unsafe { &*codec };
    let h = unsafe { &*in_header };
    let payload_slice = if payload.is_null() || payload_len == 0 {
        &[]
    } else {
        unsafe { slice::from_raw_parts(payload, payload_len) }
    };
    let out_slice = unsafe { slice::from_raw_parts_mut(out_buf, out_cap) };

    let msg_type = match MessageType::from_u16(h.message_type) {
        Ok(m) => m,
        Err(e) => {
            return if let Some(c) = e.code() {
                -(c.as_u16() as i32)
            } else {
                -0x0004
            };
        }
    };

    let fixed_h = FixedHeader {
        magic: h.magic,
        version: h.version,
        flags: FrameFlags(h.flags),
        message_type: msg_type,
        header_length: h.header_length,
        stream_id: h.stream_id,
        sequence: h.sequence,
        timestamp_ns: h.timestamp_ns,
        publisher_id: h.publisher_id,
        payload_length: payload_len as u32,
        checksum: h.checksum,
    };

    let owned_frame = OwnedFrame {
        header: fixed_h,
        extension_bytes: Vec::new(),
        payload: payload_slice.to_vec(),
    };

    match codec_ref.inner.encode(&owned_frame, out_slice) {
        Ok(len) => {
            unsafe {
                *out_len = len;
            }
            0
        }
        Err(e) => {
            if let Some(c) = e.code() {
                -(c.as_u16() as i32)
            } else {
                -0x0002
            }
        }
    }
}
