use std::collections::HashMap;
use std::fs;
use std::path::Path;

use rtdp_core::error::ErrorCode;
use rtdp_core::serialization::{CodecConfig, FrameCodec};
use serde::Deserialize;

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct GoldenVector {
    description: String,
    hex: String,
    expected_result: String,
    expected_error: Option<String>,
}

fn main() {
    println!("============================================================");
    println!(" RTDP/1 Conformance Test Suite (Normative v1.1 Baseline)");
    println!("============================================================");

    let candidates = [
        "protocol/test-vectors/frames.json",
        "../protocol/test-vectors/frames.json",
        "../../protocol/test-vectors/frames.json",
    ];

    let path_str = candidates
        .iter()
        .find(|p| Path::new(p).exists())
        .expect("Cannot find protocol/test-vectors/frames.json");

    let content = fs::read_to_string(path_str).expect("Read frames.json");
    let vectors: HashMap<String, GoldenVector> = serde_json::from_str(&content).expect("Parse JSON");

    let codec = FrameCodec::new(CodecConfig {
        max_frame_size: 1472,
        validate_checksum: true,
    });

    let mut passed = 0;
    let mut failed = 0;

    for (name, vec) in &vectors {
        print!("Testing vector [{:<35}] ... ", name);
        let mut raw_bytes = hex_to_bytes(&vec.hex);
        let res = codec.decode(&mut raw_bytes);

        if vec.expected_result == "OK" {
            match res {
                Ok(frame) => {
                    println!("PASSED (Valid message type: {:?})", frame.header.message_type);
                    passed += 1;
                }
                Err(e) => {
                    println!("FAILED! Expected OK, got error: {:?}", e);
                    failed += 1;
                }
            }
        } else {
            match res {
                Ok(f) => {
                    println!("FAILED! Expected error, but decoded: {:?}", f);
                    failed += 1;
                }
                Err(e) => {
                    let err_code = e.code().expect("Expected error code");
                    let expected_str = vec.expected_error.as_deref().unwrap_or("");
                    let matches = match expected_str {
                        "MALFORMED_FRAME" => err_code == ErrorCode::MalformedFrame,
                        "VERSION_MISMATCH" => err_code == ErrorCode::VersionMismatch,
                        "CHECKSUM_FAILED" => err_code == ErrorCode::ChecksumFailed,
                        "UNSUPPORTED_EXTENSION" => err_code == ErrorCode::UnsupportedExtension,
                        "FRAME_TOO_LARGE" => err_code == ErrorCode::FrameTooLarge,
                        _ => false,
                    };

                    if matches {
                        println!("PASSED (Correctly rejected with {})", err_code);
                        passed += 1;
                    } else {
                        println!("FAILED! Expected {}, got {}", expected_str, err_code);
                        failed += 1;
                    }
                }
            }
        }
    }

    println!("------------------------------------------------------------");
    println!("Conformance Summary: {} passed, {} failed out of {} vectors", passed, failed, vectors.len());
    println!("------------------------------------------------------------");

    if failed > 0 {
        std::process::exit(1);
    }
}

fn hex_to_bytes(s: &str) -> Vec<u8> {
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
        .collect()
}
