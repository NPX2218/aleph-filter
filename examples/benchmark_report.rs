//! Standalone benchmark runner that produces structured JSON and console summary.
//!
//! Run with:
//!   cargo run --release --example benchmark_report
//!
//! Outputs:
//!   benches/results/benchmark_results.json
//!   Pretty-printed console table

use aleph_filter::AlephFilter;
use bloomfilter::Bloom;
use cuckoofilter::CuckooFilter;
use std::collections::hash_map::DefaultHasher;
use std::time::Instant;
use xorf::{Filter as XorFilterTrait, Xor8};

use serde::Serialize;

// Result types

#[derive(Serialize, Clone)]
struct BenchResult {
    filter: String,
    operation: String,
    n: usize,
    total_ns: u128,
    per_op_ns: f64,
    ops_per_sec: f64,
}

#[derive(Serialize)]
struct FprResult {
    filter: String,
    n: usize,
    target_fpr: f64,
    actual_fpr: f64,
    false_positives: usize,
    total_queries: usize,
}

#[derive(Serialize)]
struct MemoryResult {
    filter: String,
    n: usize,
    bits_per_item: f64,
    notes: String,
}

#[derive(Serialize)]
struct FeatureMatrix {
    filter: String,
    supports_insert: bool,
    supports_delete: bool,
    supports_expand: bool,
    stable_fpr: bool,
    lookup_complexity: String,
}

#[derive(Serialize)]
struct BenchmarkReport {
    timestamp: String,
    system_info: String,
    rust_version: String,
    benchmarks: Vec<BenchResult>,
    fpr_results: Vec<FprResult>,
    memory_results: Vec<MemoryResult>,
    feature_matrix: Vec<FeatureMatrix>,
}

// Key generation

fn generate_keys(n: usize) -> Vec<Vec<u8>> {
    (0..n).map(|i| format!("key_{}", i).into_bytes()).collect()
}

fn generate_miss_keys(n: usize) -> Vec<Vec<u8>> {
    (0..n)
        .map(|i| format!("miss_{}", i).into_bytes())
        .collect()
}

// Timing helpers — runs the closure `iters` times and returns median ns

fn time_op<F: FnMut()>(mut f: F, iters: usize) -> u128 {
    let mut durations = Vec::with_capacity(iters);
    for _ in 0..iters {
        let start = Instant::now();
        f();
        durations.push(start.elapsed().as_nanos());
    }
    durations.sort();
    durations[durations.len() / 2] // median
}

// Main

fn main() {
    let sizes = vec![1_000, 10_000, 100_000];
    let iters = 5; // repetitions per measurement (take median)
    let mut results: Vec<BenchResult> = Vec::new();
    let mut fpr_results: Vec<FprResult> = Vec::new();
    let mut memory_results: Vec<MemoryResult> = Vec::new();

    println!("Aleph Filter Comparative Benchmark Suite");
    println!();

    for &n in &sizes {
        let keys = generate_keys(n);
        let miss = generate_miss_keys(n);

        println!("Dataset size N = {:>7}", n);

        // =====================================================================
        // INSERT
        // =====================================================================
        println!("  INSERT");

        // Aleph
        let ns = time_op(
            || {
                let mut f = AlephFilter::new(n, 0.01);
                for k in &keys {
                    f.insert(k);
                }
            },
            iters,
        );
        let per_op = ns as f64 / n as f64;
        results.push(BenchResult {
            filter: "Aleph".into(),
            operation: "insert".into(),
            n,
            total_ns: ns,
            per_op_ns: per_op,
            ops_per_sec: 1e9 / per_op,
        });
        println!("    Aleph    {:>10.1} ns/op  ({:.2}M ops/s)", per_op, 1e9 / per_op / 1e6);

        // Bloom
        let ns = time_op(
            || {
                let mut f = Bloom::new_for_fp_rate(n, 0.01);
                for k in &keys {
                    f.set(k);
                }
            },
            iters,
        );
        let per_op = ns as f64 / n as f64;
        results.push(BenchResult {
            filter: "Bloom".into(),
            operation: "insert".into(),
            n,
            total_ns: ns,
            per_op_ns: per_op,
            ops_per_sec: 1e9 / per_op,
        });
        println!("    Bloom    {:>10.1} ns/op  ({:.2}M ops/s)", per_op, 1e9 / per_op / 1e6);

        // Cuckoo
        let ns = time_op(
            || {
                let mut f = CuckooFilter::<DefaultHasher>::with_capacity(n);
                for k in &keys {
                    let _ = f.add(k);
                }
            },
            iters,
        );
        let per_op = ns as f64 / n as f64;
        results.push(BenchResult {
            filter: "Cuckoo".into(),
            operation: "insert".into(),
            n,
            total_ns: ns,
            per_op_ns: per_op,
            ops_per_sec: 1e9 / per_op,
        });
        println!("    Cuckoo   {:>10.1} ns/op  ({:.2}M ops/s)", per_op, 1e9 / per_op / 1e6);

        // Xor8 (construction)
        let u64_keys: Vec<u64> = (0..n as u64).collect();
        let ns = time_op(
            || {
                let _f = Xor8::from(&u64_keys);
            },
            iters,
        );
        let per_op = ns as f64 / n as f64;
        results.push(BenchResult {
            filter: "Xor8".into(),
            operation: "build".into(),
            n,
            total_ns: ns,
            per_op_ns: per_op,
            ops_per_sec: 1e9 / per_op,
        });
        println!("    Xor8     {:>10.1} ns/op  ({:.2}M ops/s)  [build]", per_op, 1e9 / per_op / 1e6);

        // =====================================================================
        // POSITIVE LOOKUP
        // =====================================================================
        println!("  LOOKUP (positive — all keys present)");

        // Aleph
        let mut af = AlephFilter::new(n, 0.01);
        for k in &keys {
            af.insert(k);
        }
        let ns = time_op(
            || {
                for k in &keys {
                    std::hint::black_box(af.contains(k));
                }
            },
            iters,
        );
        let per_op = ns as f64 / n as f64;
        results.push(BenchResult {
            filter: "Aleph".into(),
            operation: "lookup_positive".into(),
            n,
            total_ns: ns,
            per_op_ns: per_op,
            ops_per_sec: 1e9 / per_op,
        });
        println!("    Aleph    {:>10.1} ns/op  ({:.2}M ops/s)", per_op, 1e9 / per_op / 1e6);

        // Bloom
        let mut bf = Bloom::new_for_fp_rate(n, 0.01);
        for k in &keys {
            bf.set(k);
        }
        let ns = time_op(
            || {
                for k in &keys {
                    std::hint::black_box(bf.check(k));
                }
            },
            iters,
        );
        let per_op = ns as f64 / n as f64;
        results.push(BenchResult {
            filter: "Bloom".into(),
            operation: "lookup_positive".into(),
            n,
            total_ns: ns,
            per_op_ns: per_op,
            ops_per_sec: 1e9 / per_op,
        });
        println!("    Bloom    {:>10.1} ns/op  ({:.2}M ops/s)", per_op, 1e9 / per_op / 1e6);

        // Cuckoo
        let mut cf = CuckooFilter::<DefaultHasher>::with_capacity(n);
        for k in &keys {
            let _ = cf.add(k);
        }
        let ns = time_op(
            || {
                for k in &keys {
                    std::hint::black_box(cf.contains(k));
                }
            },
            iters,
        );
        let per_op = ns as f64 / n as f64;
        results.push(BenchResult {
            filter: "Cuckoo".into(),
            operation: "lookup_positive".into(),
            n,
            total_ns: ns,
            per_op_ns: per_op,
            ops_per_sec: 1e9 / per_op,
        });
        println!("    Cuckoo   {:>10.1} ns/op  ({:.2}M ops/s)", per_op, 1e9 / per_op / 1e6);

        // Xor8
        let xf = Xor8::from(&u64_keys);
        let ns = time_op(
            || {
                for k in &u64_keys {
                    std::hint::black_box(xf.contains(k));
                }
            },
            iters,
        );
        let per_op = ns as f64 / n as f64;
        results.push(BenchResult {
            filter: "Xor8".into(),
            operation: "lookup_positive".into(),
            n,
            total_ns: ns,
            per_op_ns: per_op,
            ops_per_sec: 1e9 / per_op,
        });
        println!("    Xor8     {:>10.1} ns/op  ({:.2}M ops/s)", per_op, 1e9 / per_op / 1e6);

        // =====================================================================
        // NEGATIVE LOOKUP
        // =====================================================================
        println!("  LOOKUP (negative — none present)");

        let ns = time_op(
            || {
                for k in &miss {
                    std::hint::black_box(af.contains(k));
                }
            },
            iters,
        );
        let per_op = ns as f64 / n as f64;
        results.push(BenchResult {
            filter: "Aleph".into(),
            operation: "lookup_negative".into(),
            n,
            total_ns: ns,
            per_op_ns: per_op,
            ops_per_sec: 1e9 / per_op,
        });
        println!("    Aleph    {:>10.1} ns/op  ({:.2}M ops/s)", per_op, 1e9 / per_op / 1e6);

        let ns = time_op(
            || {
                for k in &miss {
                    std::hint::black_box(bf.check(k));
                }
            },
            iters,
        );
        let per_op = ns as f64 / n as f64;
        results.push(BenchResult {
            filter: "Bloom".into(),
            operation: "lookup_negative".into(),
            n,
            total_ns: ns,
            per_op_ns: per_op,
            ops_per_sec: 1e9 / per_op,
        });
        println!("    Bloom    {:>10.1} ns/op  ({:.2}M ops/s)", per_op, 1e9 / per_op / 1e6);

        let ns = time_op(
            || {
                for k in &miss {
                    std::hint::black_box(cf.contains(k));
                }
            },
            iters,
        );
        let per_op = ns as f64 / n as f64;
        results.push(BenchResult {
            filter: "Cuckoo".into(),
            operation: "lookup_negative".into(),
            n,
            total_ns: ns,
            per_op_ns: per_op,
            ops_per_sec: 1e9 / per_op,
        });
        println!("    Cuckoo   {:>10.1} ns/op  ({:.2}M ops/s)", per_op, 1e9 / per_op / 1e6);

        let miss_u64: Vec<u64> = (n as u64..2 * n as u64).collect();
        let ns = time_op(
            || {
                for k in &miss_u64 {
                    std::hint::black_box(xf.contains(k));
                }
            },
            iters,
        );
        let per_op = ns as f64 / n as f64;
        results.push(BenchResult {
            filter: "Xor8".into(),
            operation: "lookup_negative".into(),
            n,
            total_ns: ns,
            per_op_ns: per_op,
            ops_per_sec: 1e9 / per_op,
        });
        println!("    Xor8     {:>10.1} ns/op  ({:.2}M ops/s)", per_op, 1e9 / per_op / 1e6);

        // =====================================================================
        // DELETE (Aleph & Cuckoo only)
        // =====================================================================
        if n <= 10_000 {
            println!("  DELETE");

            let ns = time_op(
                || {
                    let mut f = AlephFilter::new(n, 0.01);
                    for k in &keys {
                        f.insert(k);
                    }
                    for k in &keys {
                        f.delete(k);
                    }
                },
                iters,
            );
            // subtract insert time approximation
            let insert_ns = results
                .iter()
                .find(|r| r.filter == "Aleph" && r.operation == "insert" && r.n == n)
                .map(|r| r.total_ns)
                .unwrap_or(0);
            let del_ns = ns.saturating_sub(insert_ns);
            let per_op = del_ns as f64 / n as f64;
            results.push(BenchResult {
                filter: "Aleph".into(),
                operation: "delete".into(),
                n,
                total_ns: del_ns,
                per_op_ns: per_op,
                ops_per_sec: if per_op > 0.0 { 1e9 / per_op } else { 0.0 },
            });
            println!("    Aleph    {:>10.1} ns/op", per_op);

            let ns = time_op(
                || {
                    let mut f = CuckooFilter::<DefaultHasher>::with_capacity(n);
                    for k in &keys {
                        let _ = f.add(k);
                    }
                    for k in &keys {
                        f.delete(k);
                    }
                },
                iters,
            );
            let insert_ns = results
                .iter()
                .find(|r| r.filter == "Cuckoo" && r.operation == "insert" && r.n == n)
                .map(|r| r.total_ns)
                .unwrap_or(0);
            let del_ns = ns.saturating_sub(insert_ns);
            let per_op = del_ns as f64 / n as f64;
            results.push(BenchResult {
                filter: "Cuckoo".into(),
                operation: "delete".into(),
                n,
                total_ns: del_ns,
                per_op_ns: per_op,
                ops_per_sec: if per_op > 0.0 { 1e9 / per_op } else { 0.0 },
            });
            println!("    Cuckoo   {:>10.1} ns/op", per_op);
            println!("    Bloom    — (not supported)");
            println!("    Xor8     — (not supported)");
        }

        // =====================================================================
        // FALSE POSITIVE RATE
        // =====================================================================
        println!("  FALSE POSITIVE RATE (target = 1%)");

        // Aleph FPR
        let fpr_queries = 100_000;
        let fpr_miss = generate_miss_keys(fpr_queries);
        let mut fp_count = 0;
        for k in &fpr_miss {
            if af.contains(k) {
                fp_count += 1;
            }
        }
        let actual_fpr = fp_count as f64 / fpr_queries as f64;
        fpr_results.push(FprResult {
            filter: "Aleph".into(),
            n,
            target_fpr: 0.01,
            actual_fpr,
            false_positives: fp_count,
            total_queries: fpr_queries,
        });
        println!("    Aleph    {:.4}%  ({} / {})", actual_fpr * 100.0, fp_count, fpr_queries);

        // Bloom FPR
        fp_count = 0;
        for k in &fpr_miss {
            if bf.check(k) {
                fp_count += 1;
            }
        }
        let actual_fpr = fp_count as f64 / fpr_queries as f64;
        fpr_results.push(FprResult {
            filter: "Bloom".into(),
            n,
            target_fpr: 0.01,
            actual_fpr,
            false_positives: fp_count,
            total_queries: fpr_queries,
        });
        println!("    Bloom    {:.4}%  ({} / {})", actual_fpr * 100.0, fp_count, fpr_queries);

        // Cuckoo FPR
        fp_count = 0;
        for k in &fpr_miss {
            if cf.contains(k) {
                fp_count += 1;
            }
        }
        let actual_fpr = fp_count as f64 / fpr_queries as f64;
        fpr_results.push(FprResult {
            filter: "Cuckoo".into(),
            n,
            target_fpr: 0.01,
            actual_fpr,
            false_positives: fp_count,
            total_queries: fpr_queries,
        });
        println!("    Cuckoo   {:.4}%  ({} / {})", actual_fpr * 100.0, fp_count, fpr_queries);

        // Xor8 FPR
        let miss_u64_fpr: Vec<u64> = (n as u64..n as u64 + fpr_queries as u64).collect();
        fp_count = 0;
        for k in &miss_u64_fpr {
            if xf.contains(k) {
                fp_count += 1;
            }
        }
        let actual_fpr = fp_count as f64 / fpr_queries as f64;
        fpr_results.push(FprResult {
            filter: "Xor8".into(),
            n,
            target_fpr: 0.01,
            actual_fpr,
            false_positives: fp_count,
            total_queries: fpr_queries,
        });
        println!("    Xor8     {:.4}%  ({} / {})", actual_fpr * 100.0, fp_count, fpr_queries);

        println!();
    }

    // =====================================================================
    // EXPANSION benchmark (Aleph only)
    // =====================================================================
    println!("EXPANSION (Aleph-only, start=64 slots to 50k inserts)");
    let exp_keys = generate_keys(50_000);
    let ns = time_op(
        || {
            let mut f = AlephFilter::new(64, 0.01);
            for k in &exp_keys {
                f.insert(k);
            }
        },
        iters,
    );
    let per_op = ns as f64 / 50_000.0;
    results.push(BenchResult {
        filter: "Aleph".into(),
        operation: "insert_with_expansion".into(),
        n: 50_000,
        total_ns: ns,
        per_op_ns: per_op,
        ops_per_sec: 1e9 / per_op,
    });
    println!("    Aleph (with expansion)  {:>10.1} ns/op  ({:.2}M ops/s)", per_op, 1e9 / per_op / 1e6);

    // Compute expansions count
    let mut f_check = AlephFilter::new(64, 0.01);
    for k in &exp_keys {
        f_check.insert(k);
    }
    println!("    Expansions triggered: {}", f_check.num_expansions());
    println!("    Final capacity: {} slots", f_check.capacity());
    println!();

    // =====================================================================
    // Memory estimates
    // =====================================================================
    println!("APPROXIMATE MEMORY (bits per item)");
    for &n in &[10_000usize] {
        // Aleph: each slot is a u64 (64 bits) + metadata u8 (8 bits) = 72 bits per slot
        // with extension slots, capacity > n
        let af = {
            let mut f = AlephFilter::new(n, 0.01);
            let keys = generate_keys(n);
            for k in &keys { f.insert(k); }
            f
        };
        let aleph_bits = (af.capacity() as f64 * 72.0) / n as f64;
        memory_results.push(MemoryResult {
            filter: "Aleph".into(),
            n,
            bits_per_item: aleph_bits,
            notes: format!("64-bit slot + 8-bit metadata × {} slots (load ≈ {:.0}%)", af.capacity(), af.load_factor() * 100.0),
        });
        println!("    Aleph    ~{:.1} bits/item  (slots={}, load={:.0}%)", aleph_bits, af.capacity(), af.load_factor() * 100.0);

        // Bloom: uses optimal k = -ln(fpr)/ln(2) ≈ 6.64 hash functions
        // optimal bits/item = -1.44 * ln(fpr) ≈ 9.6 bits/item
        let bloom_bpi = -1.44 * (0.01f64).ln();
        memory_results.push(MemoryResult {
            filter: "Bloom".into(),
            n,
            bits_per_item: bloom_bpi,
            notes: "Theoretical optimal: -1.44·ln(fpr)".into(),
        });
        println!("    Bloom    ~{:.1} bits/item  (theoretical optimal)", bloom_bpi);

        // Cuckoo: ~12 bits/item typical for 1% FPR (4 entries/bucket × 12-bit fp)
        let cuckoo_bpi = 12.0;
        memory_results.push(MemoryResult {
            filter: "Cuckoo".into(),
            n,
            bits_per_item: cuckoo_bpi,
            notes: "Typical: 12-bit fingerprint, 4 entries/bucket".into(),
        });
        println!("    Cuckoo   ~{:.1} bits/item  (typical)", cuckoo_bpi);

        // Xor8: ~9.84 bits/item (1.23 × 8 bits)
        let xor8_bpi = 1.23 * 8.0;
        memory_results.push(MemoryResult {
            filter: "Xor8".into(),
            n,
            bits_per_item: xor8_bpi,
            notes: "8-bit fingerprint × 1.23 overhead factor".into(),
        });
        println!("    Xor8     ~{:.1} bits/item  (8-bit fp × 1.23)", xor8_bpi);
    }
    println!();

    // =====================================================================
    // Feature matrix
    // =====================================================================
    let features = vec![
        FeatureMatrix {
            filter: "Bloom".into(),
            supports_insert: true,
            supports_delete: false,
            supports_expand: false,
            stable_fpr: false,
            lookup_complexity: "O(k)".into(),
        },
        FeatureMatrix {
            filter: "Cuckoo".into(),
            supports_insert: true,
            supports_delete: true,
            supports_expand: false,
            stable_fpr: false,
            lookup_complexity: "O(1)".into(),
        },
        FeatureMatrix {
            filter: "Xor8".into(),
            supports_insert: false,
            supports_delete: false,
            supports_expand: false,
            stable_fpr: true,
            lookup_complexity: "O(1)".into(),
        },
        FeatureMatrix {
            filter: "Aleph".into(),
            supports_insert: true,
            supports_delete: true,
            supports_expand: true,
            stable_fpr: true,
            lookup_complexity: "O(1)*".into(),
        },
    ];

    // =====================================================================
    // Write JSON report
    // =====================================================================
    let report = BenchmarkReport {
        timestamp: chrono_now(),
        system_info: get_system_info(),
        rust_version: env!("CARGO_PKG_VERSION").to_string(),
        benchmarks: results,
        fpr_results,
        memory_results,
        feature_matrix: features,
    };

    let json = serde_json::to_string_pretty(&report).expect("Failed to serialize");
    let dir = "benches/results";
    std::fs::create_dir_all(dir).expect("Failed to create results dir");
    let path = format!("{}/benchmark_results.json", dir);
    std::fs::write(&path, &json).expect("Failed to write JSON");
    println!("✅ Results written to {}", path);
}

fn chrono_now() -> String {
    // Simple timestamp without chrono dependency
    use std::process::Command;
    let output = Command::new("date")
        .arg("+%Y-%m-%dT%H:%M:%S%z")
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_else(|_| "unknown".into());
    output
}

fn get_system_info() -> String {
    use std::process::Command;
    let uname = Command::new("uname")
        .arg("-a")
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_else(|_| "unknown".into());
    uname
}
