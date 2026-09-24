use std::fs;
use std::path::Path;
use serde::Deserialize;
use std::collections::HashMap;

use rtdp_core::error::ErrorCode;
use rtdp_core::protocol::header::MessageType;
use rtdp_core::serialization::{CodecConfig, FrameCodec};

#[derive(Debug, Deserialize)]
struct GoldenVector {
    description: String,
    hex: String,
    expected_result: String,
    expected_error: Option<String>,
}

#[test]
fn test_all_golden_vectors() {
    let manifest_path = Path::new("../protocol/test-vectors/frames.json");
    assert!(manifest_path.exists(), "Manifest file not found: {:?}", manifest_path);

    let content = fs::read_to_string(manifest_path).expect("Read frames.json");
    let vectors: HashMap<String, GoldenVector> = serde_json::from_str(&content).expect("Parse JSON");

    let codec = FrameCodec::new(CodecConfig {
        max_frame_size: 1472,
        validate_checksum: true,
    });

    for (name, vec) in &vectors {
        let mut raw_bytes = hex_to_bytes(&vec.hex);
        let res = codec.decode(&mut raw_bytes);

        if vec.expected_result == "OK" {
            assert!(
                res.is_ok(),
                "Vector '{}' ({}) expected OK, got err: {:?}",
                name,
                vec.description,
                res.err()
            );
            let frame = res.unwrap();
            match name.as_str() {
                "valid_data" => {
                    assert_eq!(frame.header.message_type, MessageType::Data);
                    assert_eq!(frame.header.stream_id, 42);
                    assert_eq!(frame.header.sequence, 101);
                    assert_eq!(frame.header.publisher_id, 1001);
                    assert_eq!(frame.payload, b"HELLO RTDP/1 ULTRA LOW LATENCY");
                }
                "valid_heartbeat" => {
                    assert_eq!(frame.header.message_type, MessageType::Heartbeat);
                    assert_eq!(frame.payload.len(), 0);
                }
                "valid_replay_request" => {
                    assert_eq!(frame.header.message_type, MessageType::ReplayRequest);
                    assert_eq!(frame.payload.len(), 24);
                }
                "valid_non_critical_extension" => {
                    assert_eq!(frame.header.header_length, 56);
                    let mut exts = frame.extensions();
                    let first = exts.next().expect("Expected 1 extension").expect("Valid extension");
                    assert_eq!(first.ext_type, 0x0001);
                    assert!(!first.is_critical);
                }
                _ => {}
            }
        } else {
            assert!(
                res.is_err(),
                "Vector '{}' ({}) expected ERROR, but succeeded: {:?}",
                name,
                vec.description,
                res.ok()
            );
            let err = res.err().unwrap();
            let err_code = err.code().expect("Expected error code");
            let expected_code_str = vec.expected_error.as_deref().unwrap_or("");
            match expected_code_str {
                "MALFORMED_FRAME" => assert_eq!(err_code, ErrorCode::MalformedFrame),
                "VERSION_MISMATCH" => assert_eq!(err_code, ErrorCode::VersionMismatch),
                "CHECKSUM_FAILED" => assert_eq!(err_code, ErrorCode::ChecksumFailed),
                "UNSUPPORTED_EXTENSION" => assert_eq!(err_code, ErrorCode::UnsupportedExtension),
                "FRAME_TOO_LARGE" => assert_eq!(err_code, ErrorCode::FrameTooLarge),
                _ => panic!("Unexpected test error: {}", expected_code_str),
            }
        }
    }
}

fn hex_to_bytes(s: &str) -> Vec<u8> {
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
        .collect()
}
