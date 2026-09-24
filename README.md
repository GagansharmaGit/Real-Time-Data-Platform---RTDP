# RTDP — Open-Source Ultra-Low-Latency Data Platform

[![License](https://img.shields.io/badge/License-Apache_2.0-blue.svg)](https://opensource.org/licenses/Apache-2.0)
[![Protocol](https://img.shields.io/badge/Protocol-RTDP%2F1-green.svg)](protocol/spec/RTDP-1.md)
[![Baseline](https://img.shields.io/badge/Baseline-v1.1%20Corrected-orange.svg)](protocol/spec/RTDP-1.md)

**RTDP** is an open binary protocol and software stack engineered for ultra-low-latency, loss-aware machine data transport with explicit delivery semantics. It is built for robotics, high-frequency telemetry, financial market data, industrial automation, autonomous systems, simulation, and real-time edge streaming.

---

## Key Highlights

* **Ultra-Low Latency**: ~23 µs p50 loopback latency in user space on commodity hardware.
* **Deterministic Wire Format**: Strict 48-byte fixed header (big-endian), 4-byte aligned, Castagnoli CRC-32C corruption integrity check, and extensible Type-Length-Value (TLV) headers.
* **Loss-Aware Recovery**: Monotonic sequence tracking, duplicate suppression by `(stream_id, publisher_id, sequence)`, and bounded selective replay via preallocated ring buffers.
* **Explicit Delivery Modes**: `BEST_EFFORT`, `LATEST`, `AT_LEAST_ONCE`, and `RELIABLE_ORDERED`.
* **Zero Hot-Path Bloat**: Zero databases, zero JSON, zero blocking filesystem calls, and zero unbounded memory allocations in the packet path.
* **C ABI Proof**: Stable C header (`rtdp.h`) and shared library for native C/C++ integration.
* **Normative Conformance**: Complete suite of golden test vectors covering valid data, heartbeats, replay requests/responses, and hostile malformed frames.

---

## Repository Layout

```text
rtdp/
├── Cargo.toml                  # Cargo workspace definition
├── LICENSE                     # Apache 2.0 license
├── README.md                   # This document
├── SECURITY.md                 # Trust boundary & threat model
├── CONTRIBUTING.md             # Developer guidelines & DCO
├── protocol/
│   ├── spec/
│   │   ├── RTDP-1.md           # Normative RTDP/1 specification
│   │   ├── error-codes.md      # Error code registry (Appendix A)
│   │   └── extension-tlv.md    # Header extension TLV rules
│   ├── wire-format/
│   │   ├── frame.md            # Byte offsets & CRC-32C specification
│   │   └── replay.md           # Loss detection & replay state machine
│   └── test-vectors/
│       ├── frames.json         # Golden byte-level test vectors
│       └── frames.bin/         # Raw binary frame artifacts
├── core/                       # rtdp-core: codec, sequence, ring buffer, delivery
├── transports/
│   └── udp/                    # rtdp-transport-udp: direct publisher & subscriber
├── cli/                        # rtdp CLI: publish, subscribe, inspect
├── c-abi/                      # C ABI shared library & interoperability proof
├── conformance/                # Automated conformance test runner
└── benchmarks/                 # Reproducible microbenchmark suite
```

---

## Wire Format Summary

The RTDP/1 fixed header occupies exactly 48 bytes:

```text
 0                   1                   2                   3
 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|          Magic (0x5254)       |    Version    |     Flags     |
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|         Message Type          |         Header Length         |
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|                           Stream ID (u64)                     |
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|                            Sequence (u64)                     |
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|                          Timestamp NS (u64)                   |
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|                          Publisher ID (u64)                   |
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|                        Payload Length (u32)                   |
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|                        CRC-32C Checksum (u32)                 |
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|                Optional Extensions (TLV, 4-byte aligned)      |
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|                            Payload ...                        |
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
```

---

## Quickstart

### 1. Build & Test All Crates
```bash
cargo build --workspace
cargo test --workspace
```

### 2. Run Conformance Suite
```bash
cargo run --bin rtdp-conformance
```

### 3. Run Benchmark Suite
```bash
cargo run --bin rtdp-bench -- -n 10000
```

### 4. Inspect a Binary Frame
```bash
cargo run --bin rtdp -- inspect protocol/test-vectors/frames.bin/valid_data.bin
```

### 5. Run Live UDP Publisher & Subscriber
Terminal 1 (Subscriber):
```bash
cargo run --bin rtdp -- subscribe --bind 127.0.0.1:9876 --publisher 127.0.0.1:9875 --stream-id 1 --mode reliable-ordered
```

Terminal 2 (Publisher):
```bash
cargo run --bin rtdp -- publish --bind 127.0.0.1:9875 --target 127.0.0.1:9876 --stream-id 1 --count 10 --interval-ms 100
```

### 6. C ABI Interoperability Proof
```bash
cargo build --package rtdp-c
clang -I./c-abi/include ./c-abi/tests/c_abi_test.c -L./target/debug -lrtdp_c -o ./target/debug/c_abi_test
./target/debug/c_abi_test
```

---

## Security Boundary
Please review [SECURITY.md](SECURITY.md). Raw UDP v0.1 is intended for trusted/test networks. CRC-32C detects accidental transmission errors, not cryptographic tampering.
