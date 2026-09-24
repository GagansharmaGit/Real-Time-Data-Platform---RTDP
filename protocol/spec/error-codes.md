# RTDP/1 Error Codes Registry

**Protocol Version**: RTDP/1 (`0x01`)  
**Status**: Normative Specification (v1.1)

---

## 1. Error Payload Structure

When an `ERROR` message (`message_type = 0x0008`) is transmitted, the frame payload is structured as follows:

```text
 0                   1                   2                   3
 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|          Error Code           |           Reserved            |
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|                    UTF-8 Diagnostic Message                   |
|                     (Up to configured max)                    |
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
```

* `error_code` (`u16`): Big-endian 16-bit unsigned integer matching this registry.
* `reserved` (`u16`): Set to `0x0000` on send; ignored on receive.
* `message` (`[u8]`): Bounded UTF-8 string describing diagnostic context. Maximum length is 256 bytes in v0.1.

---

## 2. Normative Registry Table

| Code (Hex) | Name | Semantics and Receiver Action |
| :--- | :--- | :--- |
| `0x0001` | `VERSION_MISMATCH` | Received frame specifies a protocol version not supported by this engine. The frame is rejected without attempting to decode version-dependent fields. |
| `0x0002` | `MALFORMED_FRAME` | Frame fails structural invariants: invalid magic (`!= 0x5254`), `header_length < 48`, unaligned header length, payload length exceeding frame bounds, or malformed TLV. |
| `0x0003` | `CHECKSUM_FAILED` | CRC-32C calculation does not match header `checksum` field. Accidental bit corruption detected; frame is dropped immediately. |
| `0x0004` | `UNSUPPORTED_MESSAGE_TYPE` | `message_type` is not recognized by the receiving node or is disabled in the active deployment profile (e.g., snapshot messages in v0.1). |
| `0x0005` | `UNSUPPORTED_EXTENSION` | An extension TLV with the critical bit set (`type & 0x8000 != 0`) is present in the header but unknown to the receiver. Frame processing aborts. |
| `0x0006` | `REPLAY_UNAVAILABLE` | The requested sequence range cannot be fulfilled because requested sequences have expired from the publisher's bounded ring buffer or were never published. |
| `0x0007` | `REPLAY_RANGE_INVALID` | A `REPLAY_REQUEST` contains invalid bounds: `from_sequence > to_sequence` or `(to_sequence - from_sequence + 1)` exceeds `max_replay_range`. |
| `0x0008` | `STREAM_NOT_FOUND` | The specified `stream_id` is unknown, unmapped, or closed. |
| `0x0009` | `PUBLISHER_NOT_AUTHORIZED` | A publication or replay response was received with an unauthorized or conflicting `publisher_id` for the stream. |
| `0x000A` | `FRAME_TOO_LARGE` | Frame size or payload length exceeds the node's configured maximum MTU/frame boundary. Prevents memory exhaustion attacks. |
| `0x000B` | `RATE_LIMITED` | A sender has exceeded request rate limits (e.g., replay request flooding). |
| `0x000C` | `BACKPRESSURE` | Internal ring buffer or dispatch queue policy rejected an incoming frame (e.g. `DROP_NEWEST` activated). |
