# RTDP Security Policy and Threat Boundary

**Protocol Version**: RTDP/1 (`0x01`)  
**Development Baseline**: v1.1

---

## 1. Security Boundary for v0.1

RTDP v0.1 is an ultra-low-latency real-time transport engine currently implemented over raw UDP.

### 1.1 CRC-32C is Integrity, Not Security
* The 32-bit CRC-32C (Castagnoli polynomial `0x1EDC6F41`) header checksum provides protection **strictly against accidental corruption** (hardware flaws, bit flips, network interface card issues).
* **CRC-32C is NOT cryptographic authentication or tamper resistance**. An on-path network adversary can modify frame payloads and recalculate valid CRC-32C values at wire rate.

### 1.2 Trust Model
* **Trusted / Private Networks Only**: Raw UDP v0.1 is designed solely for trusted local area networks (LANs), private VPC subnets, or controlled device-to-device fabrics.
* **No On-Path Attacker Defense**: Senders are not authenticated at the UDP data plane in v0.1. Senders and receivers on public networks without an external secure tunnel (such as IPsec, WireGuard, or future QUIC/TLS profiles) are subject to spoofing, eavesdropping, and man-in-the-middle attacks.

### 1.3 Resource Exhaustion Mitigations
To protect against packet-flood and memory exhaustion vulnerabilities, the RTDP engine enforces:
1. **Configured Maximum Frame Size**: Frames exceeding `max_frame_size` (default `1472` bytes) are rejected early with `FRAME_TOO_LARGE (0x000A)`.
2. **Zero Untrusted Allocations**: No heap memory is allocated based on unverified wire-provided lengths.
3. **Bounded Replay Limits**: `max_replay_range` and `max_outstanding_requests` prevent replay amplification attacks.
4. **Bounded Buffers**: Ring buffers and dispatch queues use fixed capacities with explicit backpressure policies.

---

## 2. Reporting Security Vulnerabilities

Please report potential security vulnerabilities to the RTDP security team via email at `security@rtdp.dev` rather than opening public GitHub issues. Please provide reproduction vectors and environment details.
