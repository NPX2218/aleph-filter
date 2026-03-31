<p align="center">
  <h1 align="center">א Aleph Filter</h1>
  <p align="center">
    <strong>To Infinity in Constant Time</strong>
  </p>
  <p align="center">
    The first Rust implementation of the Aleph Filter — an infinitely expandable<br>
    probabilistic data structure with O(1) insert, query, and delete.
  </p>
  <p align="center">
    <a href="https://www.rust-lang.org/"><img src="https://img.shields.io/badge/rust-1.70%2B-E05D44?style=flat-square&logo=rust&logoColor=white" alt="Rust 1.70+"></a>
    <a href="LICENSE"><img src="https://img.shields.io/badge/license-MIT-4CC61E?style=flat-square" alt="MIT License"></a>
    <a href="https://arxiv.org/abs/2404.04703"><img src="https://img.shields.io/badge/paper-VLDB%202024-3B82F6?style=flat-square" alt="VLDB 2024"></a>
    <a href="https://github.com/NPX2218/aleph-filter"><img src="https://img.shields.io/badge/github-NPX2218-181717?style=flat-square&logo=github" alt="GitHub"></a>
  </p>
</p>

---

## What is the Aleph Filter?

The **Aleph Filter** is a probabilistic data structure for approximate membership queries, introduced in the [VLDB 2024 paper](https://arxiv.org/abs/2404.04703) by Dayan, Bercea, and Pagh. It answers a deceptively simple question:

> *"Is this element in the set?"*

Like Bloom and Cuckoo filters, it may produce false positives but **never** false negatives. Unlike them, it can **expand indefinitely** without rebuilding, sacrificing one fingerprint bit per expansion to double its capacity, all while maintaining O(1) operations.

### Why does this matter?

| Problem | Traditional Filters | Aleph Filter |
|---------|-------------------|--------------|
| Dataset grows beyond capacity | Rebuild from scratch | Expands automatically |
| Need to delete items | ❌ (Bloom) or limited (Cuckoo) | ✅ Full support |
| FPR degrades as load increases | Yes | Stable by design |
| Query cost grows with expansions | O(log N) (InfiniFilter) | **O(1) always** |

## Quick Start

```rust
use aleph_filter::AlephFilter;

// Create a filter for ~1000 items with 1% false positive rate
let mut filter = AlephFilter::new(1000, 0.01);

// Insert
filter.insert(b"hello");
filter.insert(b"world");

// Query
assert!(filter.contains(b"hello"));   // true  — definitely in the set
assert!(!filter.contains(b"nope"));   // false — definitely NOT in the set

// Delete
filter.delete(b"hello");
assert!(!filter.contains(b"hello"));  // gone

// The filter expands automatically as you insert more items.
// No need to pre-size or rebuild!
for i in 0..100_000 {
    filter.insert(format!("key_{}", i).as_bytes());
}
```

## Performance

Benchmarked on Apple M2 (ARM64), Rust 1.93.0, `--release` mode. All filters targeting 1% FPR.

### Throughput (ns/op, lower is better)

```
INSERT (N = 10,000)
  Aleph    ██████████████████████████████████████████ 71.9 ns    ← fastest dynamic
  Bloom    █████████████████████████████████████████████████████████████████████████████████████████████████████  193.7 ns
  Cuckoo   ████████████████████████████████████████████████████████████████████████████████████████████████████████████████████████████████████████████████████████  303.7 ns

NEGATIVE LOOKUP (N = 10,000)
  Aleph    ██████████████████████████  44.8 ns    ← fastest dynamic
  Bloom    ████████████████████████████████████████████████████████████████████████████████████████  150.4 ns
  Cuckoo   ██████████████████████████████████████████████████████████████████████████████████████████████████████  170.6 ns

DELETE (N = 10,000)
  Aleph    ██████████████████████████████████████████████████████  101.6 ns    ← 3× faster
  Cuckoo   █████████████████████████████████████████████████████████████████████████████████████████████████████████████████████████████████████████████████████  287.6 ns
  Bloom    — (not supported)
```

### False Positive Rate (target = 1.00%)

| N | Aleph | Bloom | Cuckoo | Xor8 |
|---|-------|-------|--------|------|
| 1K | **0.39%** ✅ | 1.04% | 2.98% ⚠️ | 0.42% |
| 10K | **0.43%** ✅ | 0.99% | 1.98% ⚠️ | 0.39% |
| 100K | **0.62%** ✅ | 1.00% | 2.47% ⚠️ | 0.41% |

> Full benchmark results & methodology: [`docs/chapters/comparison-and-conclusion.tex`](docs/chapters/comparison-and-conclusion.tex)

### Run Benchmarks Yourself

```bash
# Quick report (~30s) — console output + JSON
cargo run --release --example benchmark_report

# Full Criterion statistical benchmarks (~15 min) — HTML reports
cargo bench --bench filter_comparison
open target/criterion/report/index.html
```

## How It Works

The Aleph Filter is built on a **quotient filter** foundation with three key innovations:

```
┌─────────────────────────────────────────────────────────────────────┐
│                        Hash Function (xxHash3)                      │
│                              64-bit hash                            │
│                    ┌──────────┬────────────────┐                    │
│                    │ quotient │  fingerprint   │                    │
│                    │ (q bits) │   (r bits)     │                    │
│                    └────┬─────┴───────┬────────┘                    │
│                         │             │                             │
│                         ▼             ▼                             │
│                   ┌─────────┐  ┌──────────────┐                     │
│                   │  slot   │  │  stored in   │                     │
│                   │  index  │  │  slot data   │                     │
│                   └─────────┘  └──────────────┘                     │
└─────────────────────────────────────────────────────────────────────┘
```

### 1. Pivot-Bit Expansion

When the filter exceeds 80% load, it **doubles** its slot count by:
- Incrementing quotient width by 1 bit (more address bits)
- Sacrificing 1 fingerprint bit per entry (slightly higher FPR)
- Using the sacrificed bit as a "pivot" to distribute entries to new slots

### 2. Void Entry Duplication

When all fingerprint bits are exhausted (after many expansions), entries become **void markers**. These are duplicated to both possible canonical slots during expansion, ensuring they're always findable in O(1).

### 3. Cluster-Aware Deletion

Deletion uses a full cluster-aware compaction algorithm that shifts entries backward to fill gaps, maintaining the quotient filter's structural invariants.

## Architecture

```
src/
├── lib.rs            # Public API & module declarations
├── hash.rs           # xxHash3 → (quotient, fingerprint) splitting
├── metadata.rs       # Per-slot flags: occupied, continuation, shifted
├── slot.rs           # 64-bit packed slots: fingerprint + length + markers
└── aleph_filter.rs   # Core filter: insert, query, delete, expand
```

| Module | Responsibility |
|--------|---------------|
| `hash` | Single xxHash3 call → split into quotient (slot address) and remainder (fingerprint) |
| `slot` | Pack fingerprint + length into 64 bits, with special void/tombstone markers |
| `metadata` | 3-bit flags per slot tracking cluster/run structure for O(1) lookups |
| `aleph_filter` | Quotient filter operations, run/cluster management, expansion logic |

## Feature Matrix

| Feature | Bloom | Cuckoo | Xor8 | **Aleph** |
|---------|:-----:|:------:|:----:|:---------:|
| Dynamic insert | ✅ | ✅ | ❌ | ✅ |
| Delete | ❌ | ✅ | ❌ | ✅ |
| Expand without rebuild | ❌ | ❌ | ❌ | ✅ |
| Stable FPR | ❌ | ❌ | ✅* | ✅ |
| O(1) lookup | O(k) | ✅ | ✅ | ✅ |
| Streaming/unbounded data | ❌ | ❌ | ❌ | ✅ |

<sub>* Xor8 is immutable — FPR is fixed at construction time</sub>

## Documentation

This project includes a comprehensive **LaTeX technical report** covering:

- Background on probabilistic filters (Bloom, Quotient, Cuckoo)
- Quotient filter expansion and fingerprint splitting
- The Aleph Filter algorithm in detail
- Our Rust implementation design decisions
- Comparative benchmarks with analysis

```bash
# Build the PDF (requires LaTeX installation)
cd docs && latexmk -pdf aleph_filter_explained.tex
```

> Pre-built PDF: [`docs/aleph_filter_explained.pdf`](docs/aleph_filter_explained.pdf)

## Testing

```bash
# Run all tests
cargo test

# Run with output (see FPR measurements, expansion traces)
cargo test -- --nocapture

# Run a specific test
cargo test test_no_false_negatives_10000
```

The test suite covers:
- ✅ No false negatives (critical invariant)
- ✅ FPR within target bounds
- ✅ Deletion correctness
- ✅ Expansion survival (all keys findable post-expand)
- ✅ Extreme expansion (50K inserts from 4 slots)
- ✅ Interleaved insert/delete/query
- ✅ Edge cases (empty keys, long keys, binary keys, duplicates)

## References

| Resource | Link |
|----------|------|
| **Paper** | [Aleph Filter: To Infinity in Constant Time](https://arxiv.org/abs/2404.04703) (VLDB 2024) |
| **Authors** | Niv Dayan, Ioana Bercea, Rasmus Pagh |
| **Java Reference** | [github.com/nivdayan/AlephFilter](https://github.com/nivdayan/AlephFilter) |
| **This Implementation** | [github.com/NPX2218/aleph-filter](https://github.com/NPX2218/aleph-filter) |

## License

[MIT](LICENSE) © 2026 Neel Bansal
