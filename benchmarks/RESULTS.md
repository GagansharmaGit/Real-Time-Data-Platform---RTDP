# RTDP/1 Microbenchmark Results & Methodology

**Protocol**: RTDP/1  
**Baseline**: v1.1  
**Date**: September 2026  
**Transport**: UDP Loopback (Direct Publisher to Subscriber)  
**Hardware / OS**: Apple Silicon (Darwin aarch64), 10 Cores  
**Build Profile**: Rust Stable 1.98.1 (`target/debug` and `--release`)

---

## 1. Benchmark Methodology

* **Test Methodology**: Synchronous one-way send/receive measurement across independent UDP sockets with warm-up.
* **Payload Sizes**: 64 B, 128 B, 256 B, 1024 B, 4096 B.
* **Metrics Captured**:
  * Latency percentiles: p50, p90, p99, p99.9, p99.99
  * Throughput: messages/sec and megabytes/sec
  * Loss Recovery: gap detection + retransmission round-trip time.

---

## 2. Benchmark Numbers

| Payload | p50 Latency | p90 Latency | p99 Latency | p99.9 Latency | p99.99 Latency | Throughput | Bandwidth |
| :--- | :--- | :--- | :--- | :--- | :--- | :--- | :--- |
| **64 B** | 23.29 µs | 25.25 µs | 31.50 µs | 70.46 µs | 102.21 µs | 42,055 msg/s | 2.57 MB/s |
| **128 B** | 23.38 µs | 25.62 µs | 31.83 µs | 81.88 µs | 87.58 µs | 41,680 msg/s | 5.09 MB/s |
| **256 B** | 24.00 µs | 26.92 µs | 34.54 µs | 70.96 µs | 79.33 µs | 40,203 msg/s | 9.82 MB/s |
| **1024 B** | 26.12 µs | 29.08 µs | 34.54 µs | 81.96 µs | 102.42 µs | 37,097 msg/s | 36.23 MB/s |
| **4096 B** | 34.08 µs | 36.79 µs | 45.79 µs | 109.25 µs | 109.71 µs | 28,604 msg/s | 111.74 MB/s |

---

## 3. How to Reproduce

Run the reproducible benchmark runner:
```bash
cargo run --release --bin rtdp-bench -- -n 100000
```
