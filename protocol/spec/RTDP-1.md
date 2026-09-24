# RTDP/1 Protocol Specification

**Status**: DEVELOPMENT BASELINE v1.1  
**Protocol Version**: `0x01` (`RTDP/1`)  
**Specification License**: CC BY 4.0  
**Implementation Target**: Linux / POSIX, Rust Reference Implementation

---

## 1. Overview

RTDP (Real-Time Data Platform) is an open binary protocol and transport stack engineered for ultra-low-latency, loss-aware machine data transport with explicit delivery semantics. It targets high-frequency telemetry, robotics, financial market data, industrial automation, autonomous systems, simulation, and real-time edge streaming.

### 1.1 Non-Zero-Loss Principle
RTDP does not pretend networks achieve physical zero packet loss. Instead, RTDP provides deterministic sequence gap detection and bounded loss recovery where the selected delivery mode requires it, without introducing blocking garbage collection, unbounded buffering, or heavy broker dependencies into the packet path.

### 1.2 v1.1 Baseline & Scope Boundaries
This document defines the normative wire protocol and state machine for **RTDP/1 v0.1**.
- **Supported in v0.1**: Fixed 48-byte header, TLV extension format, UDP unicast transport, strict frame validation, sequence monotonicity, duplicate suppression, bounded selective replay, and explicit delivery modes (`BEST_EFFORT`, `LATEST`, `AT_LEAST_ONCE`, `RELIABLE_ORDERED`).
- **Deferred (Non-Goals for v0.1)**:
  - Relay / Broker daemon (Phase 3)
  - Snapshots (`SNAPSHOT_PLUS_LIVE`, Phase 2)
  - Protocol-level fragmentation (reserved flags only)
  - Multi-writer streams (one active publisher per stream in v0.1)
  - Dynamic transport negotiation / QUIC / DPDK / RDMA / Shared Memory

---

## 2. Terminology and Invariants

* **Stream**: A logical, continuous sequence of machine data identified by a 64-bit unsigned integer (`stream_id`). Stream name-to-ID mappings are managed out-of-band by configuration or discovery.
* **Publisher**: The authorized sender for a stream. In v0.1, exactly **one active publisher** exists per `stream_id`.
* **Publisher Identity (`publisher_id`)**: A 64-bit opaque identifier representing the current publisher sequence namespace. Any publisher restart or recovery event that resets or rewinds the sequence number **must** increment or randomize its `publisher_id`.
* **Sequence**: A 64-bit unsigned integer (`sequence`) starting at `1` and monotonically incrementing by `1` for each accepted `DATA` frame. Sequence numbers must never silently wrap or reset within the same `publisher_id`.
* **Duplicate Detection Key**: The 3-tuple `(stream_id, publisher_id, sequence)` uniquely identifies a single `DATA` publication.
* **Bounded Allocation**: No implementation may allocate unbounded memory based on wire-provided lengths. All buffers, queue depths, and frame sizes are bounded at initialization.

---

## 3. Frame Layout

All multi-byte numeric fields are encoded in **network byte order (big-endian)**. All frame headers are 4-byte aligned.

```text
 0                   1                   2                   3
 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|          Magic (0x5254)       |    Version    |     Flags     |
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|         Message Type          |         Header Length         |
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|                                                               |
+                           Stream ID                           +
|                                                               |
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|                                                               |
+                            Sequence                           +
|                                                               |
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|                                                               |
+                          Timestamp NS                         +
|                                                               |
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|                                                               |
+                          Publisher ID                         +
|                                                               |
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|                        Payload Length                         |
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|                        CRC-32C Checksum                       |
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|                Optional Header Extensions (TLV) ...           |
|                (Present only if Header Length > 48)           |
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|                            Payload                            |
|                     (Payload Length bytes)                    |
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
```

### 3.1 Field Definitions

| Offset | Type | Field | Description |
| :--- | :--- | :--- | :--- |
| `0` | `u16` | `magic` | Must be `0x5254` (`"RT"` in ASCII). Reject immediately if incorrect. |
| `2` | `u8` | `version` | Protocol version. Must be `0x01` for RTDP/1. |
| `3` | `u8` | `flags` | Bitmask:<br>&bull; Bit 0: `FRAGMENTED` (reserved, must be 0 in v0.1)<br>&bull; Bit 1: `LAST_FRAGMENT` (reserved, must be 0 in v0.1)<br>&bull; Bit 2: `COMPRESSED` (reserved, must be 0 in v0.1)<br>&bull; Bits 3–7: Reserved (must be sent as 0) |
| `4` | `u16` | `message_type` | Enumerated message type (Section 4). |
| `6` | `u16` | `header_length` | Total length of header in bytes including extensions. Minimum `48`. Must be a multiple of 4. |
| `8` | `u64` | `stream_id` | Unique 64-bit stream identifier. |
| `16` | `u64` | `sequence` | Monotonic 64-bit sequence number starting at 1 for DATA frames. |
| `24` | `u64` | `timestamp_ns` | UTC Unix epoch time in nanoseconds at producer event creation or publication. |
| `32` | `u64` | `publisher_id` | 64-bit publisher namespace identity. |
| `40` | `u32` | `payload_length` | Length of the payload in bytes. |
| `44` | `u32` | `checksum` | CRC-32C (Castagnoli, polynomial `0x1EDC6F41`) calculated across the entire frame (fixed header + extensions + payload) with this field zeroed (`0x00000000`). |
| `48` | `[u8]` | `extensions` | Zero or more 4-byte aligned TLV blocks when `header_length > 48`. |
| `HL` | `[u8]` | `payload` | Exactly `payload_length` bytes beginning at offset `header_length`. |

---

## 4. Message Types

| Value | Name | Semantics & Payload Schema |
| :--- | :--- | :--- |
| `0x0001` | `DATA` | Normal streaming payload. Fixed header carries `sequence >= 1`. |
| `0x0002` | `HEARTBEAT` | Transport liveness ping. `payload_length = 0`. |
| `0x0003` | `REPLAY_REQUEST` | Explicit range request. Payload: `request_id (u64)` + `from_sequence (u64)` + `to_sequence (u64)`. |
| `0x0004` | `REPLAY_RESPONSE` | Single replayed DATA message. Header echoes original metadata (`stream_id`, `sequence`, `timestamp_ns`, `publisher_id`). Payload: `request_id (u64)` + original DATA payload bytes. |
| `0x0005` | `REPLAY_UNAVAILABLE` | Explicit notification of gap beyond retention. Payload: `request_id (u64)` + `first_unavailable_sequence (u64)` + `last_unavailable_sequence (u64)`. |
| `0x0006` | `SNAPSHOT_REQUEST` | Reserved for Phase 2. Rejected in v0.1 with `UNSUPPORTED_MESSAGE_TYPE`. |
| `0x0007` | `SNAPSHOT_RESPONSE` | Reserved for Phase 2. Rejected in v0.1 with `UNSUPPORTED_MESSAGE_TYPE`. |
| `0x0008` | `ERROR` | Generic protocol diagnostic. Payload: `error_code (u16)` + `reserved (u16)` + UTF-8 error string. |
| `0xFF00-0xFFFF`| `EXPERIMENTAL` | Vendor/experimental profile extensions. |

---

## 5. Strict Parser and Validation Rules

A compliant receiver **must** process incoming frames in the following strict order:

1. **Length Check 1**: Ensure received packet buffer is at least 48 bytes. Else reject with `MALFORMED_FRAME (0x0002)`.
2. **Magic Check**: Verify bytes 0..1 equal `0x5254`. Else reject immediately without further processing.
3. **Version Check**: Verify byte 2 equals `0x01`. Else reject with `VERSION_MISMATCH (0x0001)`.
4. **Header Length Check**: Verify `header_length >= 48`, `(header_length % 4) == 0`, and `header_length <= packet_length`. Else reject with `MALFORMED_FRAME (0x0002)`.
5. **Payload Length Check**: Verify `header_length + payload_length == packet_length`. If packet exceeds configured `max_frame_size`, reject with `FRAME_TOO_LARGE (0x000A)`. Else reject mismatch with `MALFORMED_FRAME (0x0002)`.
6. **Extensions Validation**: If `header_length > 48`, parse TLVs starting at byte 48 up to `header_length`.
   - Each TLV must fit within `header_length`.
   - Each TLV must be padded to a 4-byte boundary.
   - Unknown extension types with the high bit set (`type & 0x8000 != 0`) must abort parsing and reject with `UNSUPPORTED_EXTENSION (0x0005)`.
   - Unknown extension types with high bit clear are safely ignored.
7. **Integrity Check**: Zero the 4 bytes at offset 44..47, calculate CRC-32C over the full frame buffer, and compare against the wire checksum. If mismatch, reject with `CHECKSUM_FAILED (0x0003)`.
8. **Message Validation**: Validate message-specific payload schemas (e.g. `REPLAY_REQUEST` must have `payload_length == 24`).
9. **Dispatch**: Hand frame to sequence tracker and deduplication filter before exposing to application layers.

---

## 6. Delivery Semantics Contract

| Mode | In-Order Delivery | Drop Behavior | Duplicate Delivery | Loss Recovery |
| :--- | :--- | :--- | :--- | :--- |
| `BEST_EFFORT` | Best effort (as received) | Allowed without recovery | Allowed (network duplicates passed through) | No automatic replay |
| `LATEST` | Drops older sequences | Stale messages discarded | Duplicates suppressed | Replaces state with newest accepted sequence |
| `AT_LEAST_ONCE` | Replays gaps | Recoverable within ring buffer | Duplicates possible (exposed to consumer) | Automatic `REPLAY_REQUEST` on gap |
| `RELIABLE_ORDERED`| Strictly ascending sequence | Buffered until gap resolved | Duplicates strictly suppressed | Bounded replay ring buffer; transitions to error if unrecoverable |
| `SNAPSHOT_PLUS_LIVE` | Disabled in v0.1 | N/A | N/A | Reserved for Phase 2 |

---

## 7. Security and Trust Model

* **CRC-32C is Integrity, Not Security**: CRC-32C protects solely against accidental hardware and bit-flip network corruption. It provides no cryptographic authentication, confidentiality, or anti-tampering properties.
* **Trust Boundary**: Raw UDP v0.1 is designed strictly for private, trusted local-area networks or controlled development topologies. On-path attackers can forge or alter frames.
* **Resource Exhaustion Defense**: Configured upper bounds (`max_frame_size`, `max_replay_range`, `max_buffered_messages`) protect against memory exhaustion attacks.
