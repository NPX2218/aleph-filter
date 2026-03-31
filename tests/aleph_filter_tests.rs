use aleph_filter::AlephFilter;

// ============================================================================
// BASIC FUNCTIONALITY
// ============================================================================

#[test]
fn test_create_empty() {
    let f = AlephFilter::new(1000, 0.01);
    assert!(f.is_empty());
    assert_eq!(f.len(), 0);
}

#[test]
fn test_insert_single() {
    let mut f = AlephFilter::new(100, 0.01);
    f.insert(b"hello");
    assert_eq!(f.len(), 1);
    assert!(!f.is_empty());
}

#[test]
fn test_query_inserted() {
    let mut f = AlephFilter::new(100, 0.01);
    f.insert(b"hello");
    assert!(f.contains(b"hello"));
}

#[test]
fn test_query_not_inserted() {
    let mut f = AlephFilter::new(100, 0.01);
    f.insert(b"hello");
    // This MIGHT return true (false positive), but usually false
    // We can't assert it's definitely false
    let _ = f.contains(b"world");
}

#[test]
fn test_multiple_inserts() {
    let mut f = AlephFilter::new(100, 0.01);

    f.insert(b"apple");
    f.insert(b"banana");
    f.insert(b"cherry");
    f.insert(b"date");
    f.insert(b"elderberry");

    assert!(f.contains(b"apple"));
    assert!(f.contains(b"banana"));
    assert!(f.contains(b"cherry"));
    assert!(f.contains(b"date"));
    assert!(f.contains(b"elderberry"));
}

// ============================================================================
// NO FALSE NEGATIVES (Critical property!)
// ============================================================================

#[test]
fn test_no_false_negatives_100() {
    let mut f = AlephFilter::new(1000, 0.01);

    for i in 0..100 {
        f.insert(format!("key_{}", i).as_bytes());
    }

    for i in 0..100 {
        assert!(
            f.contains(format!("key_{}", i).as_bytes()),
            "False negative for key_{}",
            i
        );
    }
}

#[test]
fn test_no_false_negatives_1000() {
    let mut f = AlephFilter::new(10000, 0.01);

    for i in 0..1000 {
        f.insert(format!("item_{}", i).as_bytes());
    }

    for i in 0..1000 {
        assert!(
            f.contains(format!("item_{}", i).as_bytes()),
            "False negative for item_{}",
            i
        );
    }
}

#[test]
fn test_no_false_negatives_10000() {
    let mut f = AlephFilter::new(50000, 0.01);

    for i in 0..10000 {
        f.insert(format!("element_{}", i).as_bytes());
    }

    for i in 0..10000 {
        assert!(
            f.contains(format!("element_{}", i).as_bytes()),
            "False negative for element_{}",
            i
        );
    }
}

// ============================================================================
// FALSE POSITIVE RATE
// ============================================================================

#[test]
fn test_fpr_approximately_correct() {
    let target_fpr = 0.01; // 1%
    let mut f = AlephFilter::new(10000, target_fpr);

    // Insert 5000 items
    for i in 0..5000 {
        f.insert(format!("inserted_{}", i).as_bytes());
    }

    // Query 10000 items that were NOT inserted
    let mut false_positives = 0;
    for i in 0..10000 {
        if f.contains(format!("not_inserted_{}", i).as_bytes()) {
            false_positives += 1;
        }
    }

    let actual_fpr = false_positives as f64 / 10000.0;
    println!(
        "Target FPR: {:.2}%, Actual FPR: {:.2}%",
        target_fpr * 100.0,
        actual_fpr * 100.0
    );

    // Allow up to 5% (generous due to randomness and expansion effects)
    assert!(
        actual_fpr < 0.05,
        "FPR too high: {:.2}% (target was {:.2}%)",
        actual_fpr * 100.0,
        target_fpr * 100.0
    );
}

// ============================================================================
// DELETION
// ============================================================================

#[test]
fn test_delete_single() {
    let mut f = AlephFilter::new(100, 0.01);

    f.insert(b"hello");
    assert!(f.contains(b"hello"));

    let deleted = f.delete(b"hello");
    assert!(deleted);
    assert!(!f.contains(b"hello"));
}

#[test]
fn test_delete_not_present() {
    let mut f = AlephFilter::new(100, 0.01);

    f.insert(b"hello");
    let deleted = f.delete(b"world");
    assert!(!deleted);
}

#[test]
fn test_delete_twice() {
    let mut f = AlephFilter::new(100, 0.01);

    f.insert(b"hello");
    assert!(f.delete(b"hello"));
    assert!(!f.delete(b"hello")); // Second delete should fail
}

#[test]
fn test_delete_multiple() {
    let mut f = AlephFilter::new(100, 0.01);

    f.insert(b"a");
    f.insert(b"b");
    f.insert(b"c");

    assert!(f.delete(b"b"));
    assert!(f.contains(b"a"));
    assert!(!f.contains(b"b"));
    assert!(f.contains(b"c"));
}

// ============================================================================
// EXPANSION
// ============================================================================

#[test]
fn test_expansion_triggers() {
    let mut f = AlephFilter::new(10, 0.01); // Very small
    let initial_capacity = f.capacity();

    // Insert enough to trigger expansion
    for i in 0..100 {
        f.insert(format!("key_{}", i).as_bytes());
    }

    assert!(
        f.capacity() > initial_capacity,
        "Filter should have expanded"
    );
    assert!(f.num_expansions() > 0);
}

#[test]
fn test_items_survive_expansion() {
    let mut f = AlephFilter::new(10, 0.01);

    for i in 0..100 {
        f.insert(format!("key_{}", i).as_bytes());
    }

    // All items must still be found after expansion
    for i in 0..100 {
        assert!(
            f.contains(format!("key_{}", i).as_bytes()),
            "Lost key_{} after expansion",
            i
        );
    }
}

#[test]
fn test_many_expansions() {
    let mut f = AlephFilter::new(8, 0.1); // Very small, low FPR bits

    // Insert enough to trigger MANY expansions
    for i in 0..10000 {
        f.insert(format!("k{}", i).as_bytes());
    }

    println!(
        "After 10k inserts: {} slots, {} expansions, {:.1}% load",
        f.capacity(),
        f.num_expansions(),
        f.load_factor() * 100.0
    );

    // All items must still be found
    for i in 0..10000 {
        assert!(
            f.contains(format!("k{}", i).as_bytes()),
            "Lost k{} after {} expansions",
            i,
            f.num_expansions()
        );
    }
}

#[test]
fn test_extreme_expansion() {
    let mut f = AlephFilter::new(4, 0.5); // Tiny, will expand a lot

    for i in 0..50000 {
        f.insert(format!("item{}", i).as_bytes());
    }

    println!(
        "Extreme test: {} expansions, {} slots",
        f.num_expansions(),
        f.capacity()
    );

    // Spot check some items
    for i in (0..50000).step_by(1000) {
        assert!(
            f.contains(format!("item{}", i).as_bytes()),
            "Lost item{}",
            i
        );
    }
}

// ============================================================================
// EDGE CASES
// ============================================================================

#[test]
fn test_empty_key() {
    let mut f = AlephFilter::new(100, 0.01);
    f.insert(b"");
    assert!(f.contains(b""));
}

#[test]
fn test_long_key() {
    let mut f = AlephFilter::new(100, 0.01);
    let long_key = vec![b'x'; 10000];
    f.insert(&long_key);
    assert!(f.contains(&long_key));
}

#[test]
fn test_binary_key() {
    let mut f = AlephFilter::new(100, 0.01);
    let binary_key = vec![0u8, 1, 2, 255, 254, 253, 0, 0, 0];
    f.insert(&binary_key);
    assert!(f.contains(&binary_key));
}

#[test]
fn test_duplicate_inserts() {
    let mut f = AlephFilter::new(100, 0.01);

    f.insert(b"hello");
    f.insert(b"hello");
    f.insert(b"hello");

    // Should still be findable
    assert!(f.contains(b"hello"));

    // len() counts all inserts (duplicates are stored)
    assert!(f.len() >= 1);
}

// ============================================================================
// STRESS TESTS
// ============================================================================

#[test]
fn test_high_load() {
    let mut f = AlephFilter::new(1000, 0.01);

    // Fill to high load factor
    for i in 0..5000 {
        f.insert(format!("stress_{}", i).as_bytes());
    }

    // Verify
    for i in 0..5000 {
        assert!(f.contains(format!("stress_{}", i).as_bytes()));
    }
}

#[test]
fn test_interleaved_operations() {
    let mut f = AlephFilter::new(1000, 0.01);

    // Insert some
    for i in 0..100 {
        f.insert(format!("a_{}", i).as_bytes());
    }

    // Delete some
    for i in 0..50 {
        f.delete(format!("a_{}", i).as_bytes());
    }

    // Insert more
    for i in 0..100 {
        f.insert(format!("b_{}", i).as_bytes());
    }

    // Verify
    for i in 0..50 {
        assert!(!f.contains(format!("a_{}", i).as_bytes()));
    }
    for i in 50..100 {
        assert!(f.contains(format!("a_{}", i).as_bytes()));
    }
    for i in 0..100 {
        assert!(f.contains(format!("b_{}", i).as_bytes()));
    }
}

#[test]
fn test_expand_trace() {
    use aleph_filter::hash::{hash_key, split_hash};

    // Small filter that will expand after ~12 inserts (16 slots * 0.9 = 14)
    let mut f = AlephFilter::new(10, 0.01);
    let h = hash_key(b"key_0");

    let r0 = f.fp_bits();
    let (q0, fp0) = split_hash(h, f.quotient_bits(), r0);
    println!("hash={:016X} q_bits={} r_bits={} slot={} fp={:X}", h, f.quotient_bits(), r0, q0, fp0);

    f.insert(b"key_0");
    assert!(f.contains(b"key_0"), "FAIL: not found after insert");

    // Insert until expansion
    let mut i = 1;
    let old_cap = f.capacity();
    loop {
        f.insert(format!("key_{}", i).as_bytes());
        i += 1;
        if f.capacity() > old_cap { break; }
        if i > 100 { break; }
    }

    let r1 = f.fp_bits();
    let (q1, fp1) = split_hash(h, f.quotient_bits(), r1);
    println!("After expand: q_bits={} r_bits={} slot={} fp={:X} expansions={}", f.quotient_bits(), r1, q1, fp1, f.num_expansions());

    let found = f.contains(b"key_0");
    println!("contains(key_0) = {}", found);
    assert!(found, "key_0 lost after expansion");
}
