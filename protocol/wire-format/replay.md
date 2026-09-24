# RTDP/1 Replay Protocol Specification

**Protocol Version**: RTDP/1 (`0x01`)  
**Status**: Normative Specification (v1.1)

---

## 1. Replay Protocol Flow

```text
Publisher (Ring Buffer)                               Subscriber (Recovery Engine)
        |                                                          |
        |  DATA (seq=1001)                                         |
        |--------------------------------------------------------->| (Delivered)
        |                                                          |
        |  DATA (seq=1002) [LOST IN TRANSIT]                       |
        |  x - - - - - - - - - - - - - - - - - - - - - - - - - - ->|
        |                                                          |
        |  DATA (seq=1003)                                         |
        |--------------------------------------------------------->| (Gap detected: missing 1002)
        |                                                          | Buffer seq 1003
        |                                                          | Allocate request_id=0xA01
        |                                                          | State -> RECOVERING
        |  REPLAY_REQUEST (req_id=0xA01, from=1002, to=1002)       |
        |<---------------------------------------------------------|
        |                                                          |
        | (Lookup seq 1002 in ring buffer)                         |
        |  REPLAY_RESPONSE (req_id=0xA01, seq=1002)                |
        |--------------------------------------------------------->| (Match request_id)
        |                                                          | Deliver seq 1002
        |                                                          | Deliver buffered seq 1003
        |                                                          | State -> LIVE
```

---

## 2. Replay Message Schemas

### 2.1 `REPLAY_REQUEST (0x0003)`
Payload Length: exactly 24 bytes.

```text
 0                   1                   2                   3
 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|                                                               |
+                    Request ID (64-bit unsigned)               +
|                                                               |
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|                                                               |
+                  From Sequence (64-bit unsigned)              +
|                                                               |
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|                                                               |
+                   To Sequence (64-bit unsigned)               +
|                                                               |
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
```

### 2.2 `REPLAY_RESPONSE (0x0004)`
Payload Length: $8 + \text{original\_payload\_length}$ bytes.

* **Header**: Replicates the original DATA frame's `stream_id`, `sequence`, `timestamp_ns`, and `publisher_id`.
* **Payload**:
  * Bytes 0..7: `request_id` (`u64`, big-endian).
  * Bytes 8..N: Original DATA payload bytes unchanged.

### 2.3 `REPLAY_UNAVAILABLE (0x0005)`
Payload Length: exactly 24 bytes.

```text
 0                   1                   2                   3
 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|                                                               |
+                    Request ID (64-bit unsigned)               +
|                                                               |
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|                                                               |
+             First Unavailable Sequence (64-bit unsigned)      +
|                                                               |
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|                                                               |
+              Last Unavailable Sequence (64-bit unsigned)      +
|                                                               |
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
```

---

## 3. Recovery State Machine & Invariants

1. **Deterministic Retry**:
   * If a `REPLAY_REQUEST` times out before receiving all requested sequences, it is re-sent with the **identical** `request_id`.
   * A configurable `max_retries` (e.g. 3) and `retry_timeout_ms` (e.g. 50ms) govern the retry loop.
2. **Idempotence**:
   * Senders retain recently served replay responses in their cache or ring buffer; duplicate incoming `REPLAY_REQUEST` packets with identical `request_id` are answered identically without state corruption.
   * Subscribers accept duplicate `REPLAY_RESPONSE` messages idempotently and suppress duplicate deliveries.
3. **Retention Expiry**:
   * If any requested sequence has been overwritten in the publisher ring buffer, the publisher responds with `REPLAY_UNAVAILABLE`.
   * Upon receiving `REPLAY_UNAVAILABLE`, the subscriber halts replay attempts, marks the stream state as `RECOVERY_FAILED`, and emits a critical notification. It must never loop indefinitely.
