# Contributing to RTDP

Thank you for your interest in contributing to the Real-Time Data Platform (RTDP).

---

## 1. Governance Baseline
* All new development work must strictly adhere to the **RTDP v1.1 Development Baseline**.
* Protocol wire format changes require an approved, versioned RFC and test vector updates.
* Fixed header offsets are immutable for the lifetime of `RTDP/1`.

---

## 2. Developer Certificate of Origin (DCO)
All commits must be signed-off under the Developer Certificate of Origin:
```bash
git commit -s -m "feat(codec): add TLV alignment validator"
```

---

## 3. Contribution Verification Checklist
Before submitting a PR:
1. `cargo test --workspace` must pass with zero failures.
2. `cargo run --bin rtdp-conformance` must pass 100% of golden vectors.
3. No per-packet heap allocations in the hot path.
4. Security boundaries must not be relaxed.
