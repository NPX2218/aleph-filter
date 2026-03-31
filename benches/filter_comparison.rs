//! Comparative benchmarks: Aleph Filter vs Bloom, Cuckoo, and Xor filters.
//!
//! Run with:
//!   cargo bench --bench filter_comparison
//!
//! Structured JSON report written to:
//!   benches/results/benchmark_results.json

use criterion::{
    black_box, criterion_group, criterion_main, BatchSize, BenchmarkId, Criterion, Throughput,
};

use aleph_filter::AlephFilter;
use bloomfilter::Bloom;
use cuckoofilter::CuckooFilter;
use std::collections::hash_map::DefaultHasher;
use xorf::{Filter as XorFilterTrait, Xor8};

// ---------------------------------------------------------------------------
// Key generation helpers
// ---------------------------------------------------------------------------

/// Generates `n` byte-string keys: "key_0", "key_1", ...
fn generate_keys(n: usize) -> Vec<Vec<u8>> {
    (0..n).map(|i| format!("key_{}", i).into_bytes()).collect()
}

/// Generates `n` byte-string keys that do NOT overlap with `generate_keys`:
/// "miss_0", "miss_1", ...
fn generate_miss_keys(n: usize) -> Vec<Vec<u8>> {
    (0..n)
        .map(|i| format!("miss_{}", i).into_bytes())
        .collect()
}

// ---------------------------------------------------------------------------
// Benchmark groups
// ---------------------------------------------------------------------------

/// Benchmark **insertion** throughput for each filter type.
fn bench_insert(c: &mut Criterion) {
    let sizes: Vec<usize> = vec![1_000, 10_000, 100_000];

    let mut group = c.benchmark_group("insert");

    for &n in &sizes {
        let keys = generate_keys(n);
        group.throughput(Throughput::Elements(n as u64));

        // -- Aleph Filter --
        group.bench_with_input(BenchmarkId::new("Aleph", n), &keys, |b, keys| {
            b.iter(|| {
                let mut f = AlephFilter::new(n, 0.01);
                for k in keys {
                    f.insert(black_box(k));
                }
            });
        });

        // -- Bloom Filter --
        group.bench_with_input(BenchmarkId::new("Bloom", n), &keys, |b, keys| {
            b.iter(|| {
                let mut f = Bloom::new_for_fp_rate(n, 0.01);
                for k in keys {
                    f.set(black_box(k));
                }
            });
        });

        // -- Cuckoo Filter --
        group.bench_with_input(BenchmarkId::new("Cuckoo", n), &keys, |b, keys| {
            b.iter(|| {
                let mut f = CuckooFilter::<DefaultHasher>::with_capacity(n);
                for k in keys {
                    let _ = f.add(black_box(k));
                }
            });
        });

        // Xor8 is static / immutable — built from a set, not insertable.
        // We benchmark its *construction* time here.
        group.bench_with_input(BenchmarkId::new("Xor8_build", n), &keys, |b, keys| {
            let u64_keys: Vec<u64> = keys
                .iter()
                .enumerate()
                .map(|(i, _)| i as u64)
                .collect();
            b.iter(|| {
                let _f = Xor8::from(black_box(&u64_keys));
            });
        });
    }
    group.finish();
}

/// Benchmark **positive lookup** (all keys present) throughput.
fn bench_lookup_positive(c: &mut Criterion) {
    let sizes: Vec<usize> = vec![1_000, 10_000, 100_000];

    let mut group = c.benchmark_group("lookup_positive");

    for &n in &sizes {
        let keys = generate_keys(n);
        group.throughput(Throughput::Elements(n as u64));

        // -- Aleph --
        {
            let mut f = AlephFilter::new(n, 0.01);
            for k in &keys {
                f.insert(k);
            }
            group.bench_with_input(BenchmarkId::new("Aleph", n), &keys, |b, keys| {
                b.iter(|| {
                    for k in keys {
                        black_box(f.contains(black_box(k)));
                    }
                });
            });
        }

        // -- Bloom --
        {
            let mut f = Bloom::new_for_fp_rate(n, 0.01);
            for k in &keys {
                f.set(k);
            }
            group.bench_with_input(BenchmarkId::new("Bloom", n), &keys, |b, keys| {
                b.iter(|| {
                    for k in keys {
                        black_box(f.check(black_box(k)));
                    }
                });
            });
        }

        // -- Cuckoo --
        {
            let mut f = CuckooFilter::<DefaultHasher>::with_capacity(n);
            for k in &keys {
                let _ = f.add(k);
            }
            group.bench_with_input(BenchmarkId::new("Cuckoo", n), &keys, |b, keys| {
                b.iter(|| {
                    for k in keys {
                        black_box(f.contains(black_box(k)));
                    }
                });
            });
        }

        // -- Xor8 --
        {
            let u64_keys: Vec<u64> = (0..n as u64).collect();
            let xf = Xor8::from(&u64_keys);
            group.bench_with_input(BenchmarkId::new("Xor8", n), &u64_keys, |b, keys| {
                b.iter(|| {
                    for k in keys {
                        black_box(xf.contains(black_box(k)));
                    }
                });
            });
        }
    }
    group.finish();
}

/// Benchmark **negative lookup** (none of the keys present) throughput.
fn bench_lookup_negative(c: &mut Criterion) {
    let sizes: Vec<usize> = vec![1_000, 10_000, 100_000];

    let mut group = c.benchmark_group("lookup_negative");

    for &n in &sizes {
        let keys = generate_keys(n);
        let miss = generate_miss_keys(n);
        group.throughput(Throughput::Elements(n as u64));

        // -- Aleph --
        {
            let mut f = AlephFilter::new(n, 0.01);
            for k in &keys {
                f.insert(k);
            }
            group.bench_with_input(BenchmarkId::new("Aleph", n), &miss, |b, miss| {
                b.iter(|| {
                    for k in miss {
                        black_box(f.contains(black_box(k)));
                    }
                });
            });
        }

        // -- Bloom --
        {
            let mut f = Bloom::new_for_fp_rate(n, 0.01);
            for k in &keys {
                f.set(k);
            }
            group.bench_with_input(BenchmarkId::new("Bloom", n), &miss, |b, miss| {
                b.iter(|| {
                    for k in miss {
                        black_box(f.check(black_box(k)));
                    }
                });
            });
        }

        // -- Cuckoo --
        {
            let mut f = CuckooFilter::<DefaultHasher>::with_capacity(n);
            for k in &keys {
                let _ = f.add(k);
            }
            group.bench_with_input(BenchmarkId::new("Cuckoo", n), &miss, |b, miss| {
                b.iter(|| {
                    for k in miss {
                        black_box(f.contains(black_box(k)));
                    }
                });
            });
        }

        // -- Xor8 --
        {
            let u64_keys: Vec<u64> = (0..n as u64).collect();
            let xf = Xor8::from(&u64_keys);
            let miss_u64: Vec<u64> = (n as u64..2 * n as u64).collect();
            group.bench_with_input(BenchmarkId::new("Xor8", n), &miss_u64, |b, miss| {
                b.iter(|| {
                    for k in miss {
                        black_box(xf.contains(black_box(k)));
                    }
                });
            });
        }
    }
    group.finish();
}

/// Benchmark **deletion** throughput (only for filters that support it).
fn bench_delete(c: &mut Criterion) {
    let sizes: Vec<usize> = vec![1_000, 10_000];

    let mut group = c.benchmark_group("delete");

    for &n in &sizes {
        let keys = generate_keys(n);
        group.throughput(Throughput::Elements(n as u64));

        // -- Aleph --
        group.bench_with_input(BenchmarkId::new("Aleph", n), &keys, |b, keys| {
            b.iter_batched(
                || {
                    let mut f = AlephFilter::new(n, 0.01);
                    for k in keys {
                        f.insert(k);
                    }
                    f
                },
                |mut f| {
                    for k in keys {
                        black_box(f.delete(black_box(k)));
                    }
                },
                BatchSize::LargeInput,
            );
        });

        // -- Cuckoo --
        group.bench_with_input(BenchmarkId::new("Cuckoo", n), &keys, |b, keys| {
            b.iter_batched(
                || {
                    let mut f = CuckooFilter::<DefaultHasher>::with_capacity(n);
                    for k in keys {
                        let _ = f.add(k);
                    }
                    f
                },
                |mut f| {
                    for k in keys {
                        black_box(f.delete(black_box(k)));
                    }
                },
                BatchSize::LargeInput,
            );
        });

        // Bloom and Xor8 do NOT support deletion — excluded.
    }
    group.finish();
}

/// Benchmark **expansion** (only Aleph supports this natively).
fn bench_expansion(c: &mut Criterion) {
    let mut group = c.benchmark_group("expansion");

    // Insert way more than initial capacity to force multiple expansions
    let n = 50_000usize;
    let keys = generate_keys(n);
    group.throughput(Throughput::Elements(n as u64));

    group.bench_with_input(BenchmarkId::new("Aleph_expand", n), &keys, |b, keys| {
        b.iter(|| {
            // Start very small so we get many expansions
            let mut f = AlephFilter::new(64, 0.01);
            for k in keys {
                f.insert(black_box(k));
            }
            black_box(f.num_expansions());
        });
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_insert,
    bench_lookup_positive,
    bench_lookup_negative,
    bench_delete,
    bench_expansion,
);
criterion_main!(benches);
