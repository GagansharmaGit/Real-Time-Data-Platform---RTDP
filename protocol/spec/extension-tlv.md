# RTDP/1 Header Extension TLV Specification

**Protocol Version**: RTDP/1 (`0x01`)  
**Status**: Normative Specification (v1.1)

---

## 1. Extension Placement

When `header_length > 48`, the bytes between offset `48` and `header_length` contain zero or more Type-Length-Value (TLV) extension records.

* No ad-hoc fields may ever be appended to the fixed 48-byte header.
* Extensions must never alter the semantics or interpretation of fixed header fields.
* The end of the final extension coincides exactly with offset `header_length`.
* The message payload immediately follows at offset `header_length`.

---

## 2. TLV Encoding Format

Each TLV extension block consists of a 4-byte header followed by `length` bytes of value data, followed by zero to three bytes of padding such that the total length of the TLV record is a multiple of 4 bytes.

```text
 0                   1                   2                   3
 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|C|           Type              |            Length             |
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|                            Value ...                          |
|                     (Length bytes, 0..65535)                  |
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|                     Zero Padding (0-3 bytes)                  |
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
```

### 2.1 Field Rules

* **Critical Bit `C` (`bit 15` / high bit of `Type`)**:
  * `C = 0` (`Type < 0x8000`): **Non-critical extension**. If a receiver does not recognize this type, it must safely skip `4 + padded_length` bytes and proceed to the next extension.
  * `C = 1` (`Type >= 0x8000`): **Critical extension**. If a receiver does not recognize this type, it must immediately abort parsing and reject the frame with error `UNSUPPORTED_EXTENSION (0x0005)`.
* **Length (`u16`)**: Big-endian unsigned integer indicating the length in bytes of the `Value` field only (excluding the 4-byte TLV header and excluding padding).
* **Padding Calculation**:
  $$\text{padded\_len} = (\text{length} + 3) \ \& \ \sim 3$$
  The total bytes occupied by the TLV is $4 + \text{padded\_len}$. Padding bytes must be set to `0x00` on transmission and ignored on receipt.

---

## 3. Registered Extensions (v0.1)

| Type (Hex) | Name | Critical | Length | Semantics |
| :--- | :--- | :--- | :--- | :--- |
| `0x0001` | `SOURCE_INGRESS_TS` | Non-critical (`0`) | 8 | 64-bit UTC nanosecond timestamp added at NIC/switch ingress. |
| `0x0002` | `CORRELATION_ID` | Non-critical (`0`) | 16 | 128-bit UUID or trace identifier for distributed tracing. |
| `0x8001` | `REQUIRED_CIPHER_SUITE` | Critical (`1`) | 4 | Reserved for Phase 5 authenticated transport profiles. |
| `0x7F00-0x7FFF` | `EXPERIMENTAL_NON_CRITICAL` | Non-critical (`0`) | Var | Vendor / prototyping experimental extensions. |
| `0xFF00-0xFFFF` | `EXPERIMENTAL_CRITICAL` | Critical (`1`) | Var | Vendor / prototyping critical extensions. |
