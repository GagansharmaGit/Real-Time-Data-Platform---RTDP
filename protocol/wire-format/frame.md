# RTDP/1 Frame Wire Format Specification

**Protocol**: RTDP/1  
**Byte Order**: Big-Endian (Network Byte Order)  
**Fixed Header Size**: 48 Bytes

---

## 1. Frame Byte Offsets and Layout

```text
Offset (Dec) | Size (Bytes) | Field Name     | Type | Description
-------------|--------------|----------------|------|-----------------------------------------
0            | 2            | magic          | u16  | 0x5254 ('R', 'T')
2            | 1            | version        | u8   | 0x01
3            | 1            | flags          | u8   | Reserved flags (0x00)
4            | 2            | message_type   | u16  | 0x0001 (DATA), 0x0002 (HEARTBEAT), etc.
6            | 2            | header_length  | u16  | Total header bytes (>= 48, 4-byte aligned)
8            | 8            | stream_id      | u64  | Stable stream identifier
16           | 8            | sequence       | u64  | Monotonic stream position
24           | 8            | timestamp_ns   | u64  | UTC Unix Epoch ns at creation
32           | 8            | publisher_id   | u64  | Active publisher identity
40           | 4            | payload_length | u32  | Size of payload in bytes
44           | 4            | checksum       | u32  | CRC-32C of frame with bytes 44..47 zeroed
48           | HL - 48      | extensions     | [u8] | Optional TLV blocks (if HL > 48)
HL           | payload_len  | payload        | [u8] | Application message payload
```

---

## 2. Checksum Algorithm (CRC-32C)

1. Checksum polynomial is Castagnoli `0x1EDC6F41` (CRC-32C, IEEE 802.3cz / iSCSI standard).
2. During computation and verification, the 4-byte `checksum` field (bytes 44..47) is set to `0x00000000`.
3. The CRC-32C is computed over the entire frame:
   $$\text{Frame}[0..48 + \text{extensions\_len} + \text{payload\_len} - 1]$$
4. The resulting 32-bit unsigned integer is written to bytes 44..47 in big-endian order.
5. On receipt, the receiver copies bytes 44..47, zeroes them in place, recomputes CRC-32C over the packet buffer, and verifies equality.

---

## 3. Frame Bounds & MTU Policy

* **MTU Safety**: In v0.1 over raw UDP, the total packet size (`header_length + payload_length`) must not exceed the path MTU minus IP/UDP header overhead (standard recommended maximum: `1472` bytes for Ethernet 1500-byte MTU, or configured jumbo frame limit).
* **Fragmentation Flag Policy**: IP-level fragmentation must be disabled (`IP_DONTFRAG` / `IP_PMTUDISC_DO`). Senders attempting to transmit frames larger than `max_frame_size` return an error `FRAME_TOO_LARGE`.
