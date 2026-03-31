//!
//! The Aleph Filter is a quotient filter variant that supports:
//! - O(1) expected insertion and lookup
//! - Tunable false positive rate (FPR)
//! - Automatic expansion with fingerprint bit sacrifice
//! - Deletion support (via tombstones for void entries)

use std::collections::HashMap;
use crate::hash::{hash_key, split_hash, fingerprint_bits_for_fpr};
use crate::metadata::SlotMetadata;
use crate::slot::Slot;

/// The main Aleph Filter data structure.
/// 
/// Stores fingerprints in a compact quotient-filter format with dynamic expansion.
/// Each element occupies one logical "slot" with data stored in `slots` and metadata in `metadata`.
#[derive(Clone)]
pub struct AlephFilter {
    /// Fingerprint + metadata storage. Length = num_slots + num_extension_slots.
    /// Stores packed fingerprints for compact memory usage.
    slots: Vec<Slot>,
    
    /// Per-slot metadata flags (occupied, continuation, shifted).
    /// Tracks structure of runs and clusters.
    metadata: Vec<SlotMetadata>,

    /// Number of logical slots (always power of 2).
    /// The "quotient" component of the quotient filter.
    num_slots: usize,
    
    /// Extra overflow slots at the end (linear, no wraparound).
    /// Prevents constant expansion and supports insertion collisions.
    num_extension_slots: usize,

    /// log2(num_slots) - used to extract quotient from hash.
    quotient_bits: u32,
    
    /// Fingerprint bits at creation time.
    /// Decreases by 1 with each `expand()` to make space for quotient expansion.
    base_fp_bits: u32,

    /// Number of times the filter has expanded.
    /// Each expansion doubles slots and sacrifices 1 fingerprint bit per entry.
    num_expansions: usize,
    
    /// Current number of distinct items inserted.
    num_items: usize,

    /// Reserved for future use (e.g., storing full hashes for deletions).
    mother_hashes: HashMap<usize, u64>,

    /// Load factor threshold. When `num_items / num_slots >= max_load_factor`, expand.
    max_load_factor: f64,
}

// PUBLIC API

impl AlephFilter {
    /// Creates a new Aleph Filter sized for `expected_items` with target `fpr`.
    /// 
    /// Automatically computes internal parameters (slots, fingerprint width) to meet the FPR.
    /// 
    /// # Arguments
    /// 
    /// * `expected_items` - Expected number of items to insert (must be > 0)
    /// * `fpr` - Target false positive rate, e.g., 0.01 for 1% (must be in (0, 1))
    /// 
    /// # Returns
    /// 
    /// A new empty AlephFilter instance
    /// 
    /// # Panics
    /// 
    /// If `expected_items <= 0` or `fpr` not in (0, 1).
    pub fn new(expected_items: usize, fpr: f64) -> Self {
        assert!(expected_items > 0);
        assert!(fpr > 0.0 && fpr < 1.0);
        let raw = (expected_items as f64 / 0.8).ceil() as usize;
        let num_slots = raw.next_power_of_two().max(8);
        let q = num_slots.trailing_zeros();
        let fp = fingerprint_bits_for_fpr(fpr);
        let ext = (q as usize) * 2;  // Match Java: power_of_two_size * 2
        return Self {
            slots: vec![Slot::empty(); num_slots + ext],
            metadata: vec![SlotMetadata::new(); num_slots + ext],
            num_slots, num_extension_slots: ext,
            quotient_bits: q, base_fp_bits: fp,
            num_expansions: 0, num_items: 0,
            mother_hashes: HashMap::new(),
            max_load_factor: 0.8,  // Match Java: fullness_threshold = 0.8
        };
    }

    /// Creates a new Aleph Filter with manual parameters.
    /// 
    /// Use this constructor when you already know the desired slot count and fingerprint width.
    /// 
    /// # Arguments
    /// 
    /// * `num_slots` - Desired number of logical slots (rounded up to next power of 2)
    /// * `remainder_bits` - Fingerprint width in bits
    /// 
    /// # Returns
    /// 
    /// A new empty AlephFilter instance
    pub fn with_params(num_slots: usize, remainder_bits: u32) -> Self {
        let num_slots = num_slots.next_power_of_two().max(8);
        let q = num_slots.trailing_zeros();
        let ext = (q as usize) * 2;  // Match Java: power_of_two_size * 2
        return Self {
            slots: vec![Slot::empty(); num_slots + ext],
            metadata: vec![SlotMetadata::new(); num_slots + ext],
            num_slots, num_extension_slots: ext,
            quotient_bits: q, base_fp_bits: remainder_bits,
            num_expansions: 0, num_items: 0,
            mother_hashes: HashMap::new(),
            max_load_factor: 0.8,  // Match Java: fullness_threshold = 0.8
        };
    }

    /// Inserts a key into the filter.
    /// 
    /// May trigger an automatic expansion if load factor exceeds threshold.
    /// Supports duplicate insertions (see contains for lookup semantics).
    /// 
    /// # Arguments
    /// 
    /// * `key` - Byte slice to insert
    /// 
    /// # Complexity
    /// 
    /// O(1) amortized expected time
    pub fn insert(&mut self, key: &[u8]) {
        if self.load_factor() >= self.max_load_factor {
            self.expand();
        }
        let hash = hash_key(key);
        let r = self.fp_bits();
        let (slot_idx, fp) = split_hash(hash, self.quotient_bits, r);
        let canonical = slot_idx as usize;
        let slot = if r == 0 {
            Slot::void_marker() // No fingerprint bits left -> void
        } else {
            Slot::new(fp, r as u8)
        };
        self.qf_insert_slot(canonical, slot);
        self.num_items += 1;
        return;
    }

    /// Checks if a key might be in the filter.
    /// 
    /// Returns `true` if the key is definitely in the filter.
    /// May return `true` for keys not inserted (false positive at rate `fpr`).
    /// Never returns `true` for keys definitely not inserted (no false negatives).
    /// 
    /// # Arguments
    /// 
    /// * `key` - Byte slice to query
    /// 
    /// # Returns
    /// 
    /// `true` if key is in filter (or hash collision), `false` if definitely not in filter
    /// 
    /// # Complexity
    /// 
    /// O(1) expected time
    pub fn contains(&self, key: &[u8]) -> bool {
        let hash = hash_key(key);
        let r = self.fp_bits();
        let (slot_idx, fp) = split_hash(hash, self.quotient_bits, r);
        let canonical = slot_idx as usize;
        return self.qf_search(fp, canonical);
    }

    /// Removes a key from the filter, if present.
    /// 
    /// Deleting void entries (exhausted fingerprints) marks them with a tombstone.
    /// Deleting regular entries compacts the run by shifting elements left.
    /// 
    /// # Arguments
    /// 
    /// * `key` - Byte slice to delete
    /// 
    /// # Returns
    /// 
    /// `true` if a matching entry was found and deleted, `false` if not found
    /// 
    /// # Complexity
    /// 
    /// O(1) expected time
    pub fn delete(&mut self, key: &[u8]) -> bool {
        let hash = hash_key(key);
        let r = self.fp_bits();
        let (slot_idx, fp) = split_hash(hash, self.quotient_bits, r);
        let canonical = slot_idx as usize;
        if self.qf_delete(fp, canonical) {
            self.num_items = self.num_items.saturating_sub(1);
            return true;
        } else {
            return false;
        }
    }

    /// Returns the number of items inserted, accounting for duplicates.
    /// 
    /// # Returns
    /// 
    /// Count of insertion operations
    #[inline] pub fn len(&self) -> usize { return self.num_items; }
    
    /// Returns `true` if the filter contains no items.
    /// 
    /// # Returns
    /// 
    /// `true` if empty, `false` otherwise
    #[inline] pub fn is_empty(&self) -> bool { return self.num_items == 0; }
    
    /// Returns the current load factor (items / slots).
    /// 
    /// # Returns
    /// 
    /// Ratio in [0, 1+) (can exceed 1 due to extension slots)
    #[inline] pub fn load_factor(&self) -> f64 { return self.num_items as f64 / self.num_slots as f64; }
    
    /// Returns the current logical slot count.
    /// 
    /// # Returns
    /// 
    /// Number of quotient-addressable slots
    #[inline] pub fn capacity(&self) -> usize { return self.num_slots; }
    
    /// Returns the number of times the filter has expanded.
    /// 
    /// Each expansion doubles slots and sacrifices 1 fingerprint bit per entry.
    /// 
    /// # Returns
    /// 
    /// Expansion count
    #[inline] pub fn num_expansions(&self) -> usize { return self.num_expansions; }
    
    /// Returns the current number of quotient bits (log2 of logical slots).
    /// 
    /// # Returns
    /// 
    /// Quotient bits
    #[inline] pub fn quotient_bits(&self) -> u32 { return self.quotient_bits; }
}

// INTERNAL HELPERS

impl AlephFilter {
    /// Returns total addressable slots (logical + extension).
    /// 
    /// Used internally to guard against out-of-bounds access.
    /// 
    /// # Returns
    /// 
    /// Total array size
    #[inline]
    fn total_slots(&self) -> usize { return self.num_slots + self.num_extension_slots; }

    /// Returns the current fingerprint width in bits.
    /// 
    /// Decreases by 1 with each expansion to make room for additional quotient bits.
    /// Uses saturating subtraction to prevent underflow.
    /// 
    /// # Returns
    /// 
    /// Fingerprint bits available (>= 0)
    #[inline]
    pub fn fp_bits(&self) -> u32 { return self.base_fp_bits.saturating_sub(self.num_expansions as u32); }

    /// Checks if a slot is completely empty (no data and no metadata flags set).
    /// 
    /// A slot is empty only when all three metadata flags (occupied, continuation, shifted)
    /// are off AND the slot data is zero.
    /// 
    /// # Arguments
    /// 
    /// * `i` - Slot index
    /// 
    /// # Returns
    /// 
    /// `true` if slot is fully empty
    #[inline]
    fn is_slot_empty(&self, i: usize) -> bool {
        return !self.metadata[i].is_occupied()
            && !self.metadata[i].is_continuation()
            && !self.metadata[i].is_shifted();
    }

    /// Atomically swaps a slot, returning the old value.
    /// 
    /// Used by insertion logic to push elements right.
    /// 
    /// # Arguments
    /// 
    /// * `i` - Slot index
    /// * `new_slot` - Value to place in slot
    /// 
    /// # Returns
    /// 
    /// Previous value at slot `i`
    fn swap_slot(&mut self, i: usize, new_slot: Slot) -> Slot {
        let old = self.slots[i];
        self.slots[i] = new_slot;
        return old;
    }
}

// QUOTIENT FILTER CORE 

impl AlephFilter {
    /// Finds the start position of the run for a given canonical slot.
    /// 
    /// The algorithm:
    /// 1. Walk backward from canonical, counting occupied slots while shifted flag is set.
    /// 2. Walk forward from initial position, counting run starts (non-continuation slots).
    /// 3. Return position of the Nth run start where N = count from phase 1.
    /// 
    /// This determines which run a canonical slot belongs to, even if shifted.
    /// 
    /// # Arguments
    /// 
    /// * `canonical` - Quotient (canonical slot index)
    /// 
    /// # Returns
    /// 
    /// Index of the run's first slot
    /// 
    fn find_run_start(&self, canonical: usize) -> usize {
        let mut pos = canonical;
        let mut skip: i64 = 1;

        // Phase 1: walk backward counting occupied slots
        while self.metadata[pos].is_shifted() {
            if self.metadata[pos].is_occupied() {
                skip += 1;
            }
            if pos == 0 { break; }
            pos -= 1;
        }

        // Phase 2: walk forward counting run starts (non-continuation)
        loop {
            if !self.metadata[pos].is_continuation() {
                skip -= 1;
                if skip == 0 {
                    return pos;
                }
            }
            pos += 1;
            if pos >= self.total_slots() { return canonical; }
        }
    }

    /// Finds a suitable starting position for a new run.
    /// 
    /// Starting from a run start, skips past the run and any continuation slots.
    /// Returns the first empty or non-continuation position where a new run can begin.
    /// 
    /// # Arguments
    /// 
    /// * `index` - Starting position (typically a run start)
    /// 
    /// # Returns
    /// 
    /// Position suitablefor inserting a new run
    /// 
    fn find_new_run_location(&self, index: usize) -> usize {
        let mut pos = index;
        if pos < self.total_slots() && !self.is_slot_empty(pos) {
            pos += 1;
        }
        while pos < self.total_slots() && self.metadata[pos].is_continuation() {
            pos += 1;
        }
        return pos;
    }


    /// Inserts a slot at its canonical position, dispatching to appropriate insertion logic.
    /// 
    /// If no run exists at canonical, creates a new run.
    /// If a run exists, inserts into the existing run.
    /// 
    /// # Arguments
    /// 
    /// * `canonical` - Quotient (canonical slot index)
    /// * `slot` - Slot data to insert (fingerprint + length)
    /// 
    /// # Returns
    /// 
    /// `true` on success, `false` if array is full
    fn qf_insert_slot(&mut self, canonical: usize, slot: Slot) -> bool {
        let does_run_exist = self.metadata[canonical].is_occupied();
        if !does_run_exist {
            return self.insert_new_run(canonical, slot);
        } else {
            let run_start = self.find_run_start(canonical);
            return self.insert_into_run(slot, run_start);
        }
    }

    /// Creates a new run for a canonical slot and inserts the slot.
    /// 
    /// Sets the occupied flag, shifts metadata, and performs a push-right cascade
    /// if the insertion position is already occupied.
    /// 
    /// # Arguments
    /// 
    /// * `canonical` - Quotient (canonical slot index)
    /// * `slot` - Slot data to insert
    /// 
    /// # Returns
    /// 
    /// `true` on success, `false` if array is full
    /// 
    fn insert_new_run(&mut self, canonical: usize, slot: Slot) -> bool {
        let run_start = self.find_run_start(canonical);
        let insert_pos = self.find_new_run_location(run_start);

        let slot_empty = self.is_slot_empty(insert_pos);

        self.metadata[canonical].set_occupied(true);
        if insert_pos != canonical {
            self.metadata[insert_pos].set_shifted(true);
        }
        self.metadata[insert_pos].set_continuation(false);

        if slot_empty {
            self.slots[insert_pos] = slot;
            return true;
        }

        // Push everything right via swap chain
        let mut current_slot = slot;
        let mut pos = insert_pos;
        let mut temp_cont = false;
        loop {
            if pos >= self.total_slots() { return false; }
            let was_empty = self.is_slot_empty(pos);
            current_slot = self.swap_slot(pos, current_slot);

            if pos > insert_pos {
                self.metadata[pos].set_shifted(true);
            }
            if pos > insert_pos {
                let cur_cont = self.metadata[pos].is_continuation();
                self.metadata[pos].set_continuation(temp_cont);
                temp_cont = cur_cont;
            }
            pos += 1;
            if was_empty { break; }
        }
        return true;
    }

    /// Inserts a slot into an existing run, preserving run structure.
    /// 
    /// Scans forward through the run, marking boundaries and performing push-right
    /// when necessary. Maintains continuation flags to mark continuation slots.
    /// 
    /// # Arguments
    /// 
    /// * `slot` - Slot data to insert
    /// * `run_start` - Index of run's first slot
    /// 
    /// # Returns
    /// 
    /// `true` on success, `false` if array is full
    /// 
    fn insert_into_run(&mut self, slot: Slot, run_start: usize) -> bool {
        let mut current_slot = slot;
        let mut pos = run_start;
        let mut finished_first_run = false;
        let mut temp_cont = false;

        loop {
            if pos >= self.total_slots() { return false; }
            let was_empty = self.is_slot_empty(pos);

            if pos > run_start {
                self.metadata[pos].set_shifted(true);
            }

            if pos > run_start && !finished_first_run && !self.metadata[pos].is_continuation() {
                finished_first_run = true;
                self.metadata[pos].set_continuation(true);
                current_slot = self.swap_slot(pos, current_slot);
            } else if finished_first_run {
                let cur_cont = self.metadata[pos].is_continuation();
                self.metadata[pos].set_continuation(temp_cont);
                temp_cont = cur_cont;
                current_slot = self.swap_slot(pos, current_slot);
            }

            pos += 1;
            if was_empty { break; }
        }
        return true;
    }

    /// Searches for a fingerprint in the run for a canonical slot.
    /// 
    /// Returns `false` early if the canonical slot is not occupied.
    /// Otherwise, locates the run and scans it for a match.
    /// 
    /// # Arguments
    /// 
    /// * `fp` - Fingerprint to search for
    /// * `canonical` - Quotient (canonical slot index)
    /// 
    /// # Returns
    /// 
    /// `true` if fingerprint is found in the run, `false` otherwise
    /// 
    fn qf_search(&self, fp: u64, canonical: usize) -> bool {
        if !self.metadata[canonical].is_occupied() {
            return false;
        }
        let run_start = self.find_run_start(canonical);
        return self.find_in_run(run_start, fp);
    }

    /// Scans a run linearly for a matching fingerprint.
    /// 
    /// Iterates from start, checking each slot against the target fingerprint.
    /// Skips tombstones and stops at the end of the run (when continuation flag is off).
    /// 
    /// # Arguments
    /// 
    /// * `start` - Starting index of run
    /// * `fp` - Fingerprint to match
    /// 
    /// # Returns
    /// 
    /// `true` if a matching fingerprint is found, `false` otherwise
    /// 
    fn find_in_run(&self, start: usize, fp: u64) -> bool {
        let mut pos = start;
        let r = self.fp_bits();
        loop {
            if self.slots[pos].is_tombstone() {
                // skip tombstones
            } else if self.slots[pos].matches(fp, r as u8) {
                return true;
            }
            pos += 1;
            if pos >= self.total_slots() || !self.metadata[pos].is_continuation() {
                break;
            }
        }
        false
    }

    /// Finds the start of the cluster containing the given index.
    ///
    /// Walks backward until finding a slot that is not shifted.
    /// Matches Java's `find_cluster_start`.
    fn find_cluster_start(&self, index: usize) -> usize {
        let mut pos = index;
        while pos > 0 && self.metadata[pos].is_shifted() {
            pos -= 1;
        }
        return pos;
    }

    /// Finds the end of the run starting at `index`.
    ///
    /// Walks forward while continuation flag is set.
    /// Matches Java's `find_run_end`.
    fn find_run_end(&self, index: usize) -> usize {
        let mut pos = index;
        while pos + 1 < self.total_slots() && self.metadata[pos + 1].is_continuation() {
            pos += 1;
        }
        return pos;
    }

    /// Deletes a fingerprint from the run for a canonical slot.
    /// 
    /// Uses the Java paper's full cluster-aware deletion:
    /// 1. Find the matching fingerprint in the run
    /// 2. Shift entries within the run backward to fill the gap
    /// 3. For each subsequent shifted run in the cluster, shift it back by one slot
    /// 4. Clear metadata at the vacated position
    /// 5. Clear occupied flag if the run is now empty
    ///
    /// This is more correct than run-local deletion because it properly
    /// compacts the entire cluster, maintaining metadata consistency.
    fn qf_delete(&mut self, fp: u64, canonical: usize) -> bool {
        if canonical >= self.num_slots {
            return false;
        }
        if !self.metadata[canonical].is_occupied() {
            return false;
        }
        let run_start = self.find_run_start(canonical);
        let r = self.fp_bits();

        // Find the matching fingerprint (search for last match, like Java's decide_which_fingerprint_to_delete)
        let mut pos = run_start;
        let mut found: Option<usize> = None;
        loop {
            if !self.slots[pos].is_tombstone() && self.slots[pos].matches(fp, r as u8) {
                found = Some(pos);
            }
            pos += 1;
            if pos >= self.total_slots() || !self.metadata[pos].is_continuation() {
                break;
            }
        }

        let del_pos = match found {
            Some(p) => p,
            None => return false,
        };

        // Void entries get tombstoned (matching Java's behavior for void deletes)
        if self.slots[del_pos].is_void() {
            self.slots[del_pos] = Slot::tombstone();
            return true;
        }

        let mut run_end = self.find_run_end(del_pos);

        // Check if this run has only one entry (will need to clear occupied flag)
        let turn_off_occupied = run_start == run_end;

        // Shift entries within this run backward to fill the gap
        for i in del_pos..run_end {
            self.slots[i] = self.slots[i + 1];
        }

        // Count continuation and non-occupied flags from cluster start to run_end
        // This tells us how far entries are shifted from their canonical positions
        let cluster_start = self.find_cluster_start(canonical);
        let mut num_shifted_count: i64 = 0;
        let mut num_non_occupied: i64 = 0;
        for i in cluster_start..=run_end {
            if self.metadata[i].is_continuation() {
                num_shifted_count += 1;
            }
            if !self.metadata[i].is_occupied() {
                num_non_occupied += 1;
            }
        }

        // Clear the vacated slot at run_end
        self.slots[run_end] = Slot::empty();
        self.metadata[run_end].set_shifted(false);
        self.metadata[run_end].set_continuation(false);

        // Now shift all subsequent runs in the cluster backward by one slot
        loop {
            // Check if there's a next run that needs shifting
            if run_end + 1 >= self.total_slots()
                || self.is_slot_empty(run_end + 1)
                || !self.metadata[run_end + 1].is_shifted()
            {
                if turn_off_occupied {
                    self.metadata[canonical].set_occupied(false);
                }
                return true;
            }

            // Find the next run
            let next_run_start = run_end + 1;
            run_end = self.find_run_end(next_run_start);

            // Check if the slot before this run is now back at its canonical position
            if self.metadata[next_run_start - 1].is_occupied()
                && num_shifted_count - num_non_occupied == 1
            {
                self.metadata[next_run_start - 1].set_shifted(false);
            } else {
                self.metadata[next_run_start - 1].set_shifted(true);
            }

            // Shift each entry in this run back by one
            for i in next_run_start..=run_end {
                self.slots[i - 1] = self.slots[i];
                if self.metadata[i].is_continuation() {
                    self.metadata[i - 1].set_continuation(true);
                }
                if !self.metadata[i].is_occupied() {
                    num_non_occupied += 1;
                }
            }
            num_shifted_count += (run_end - next_run_start) as i64;

            // Clear the vacated slot
            self.slots[run_end] = Slot::empty();
            self.metadata[run_end].set_shifted(false);
            self.metadata[run_end].set_continuation(false);
        }
    }
}

// EXPANSION 
impl AlephFilter {
    /// Doubles the filter size and re-hashes all entries.
    /// 
    /// During expansion:
    /// 1. Collects all entries using `iterate_entries()`
    /// 2. Doubles logical slots and increments quotient bits
    /// 3. Sacrifices 1 fingerprint bit per entry (`fp_bits` decreases)
    /// 4. Re-inserts entries using pivot bit to determine new canonical slot
    /// 5. Consolidates void markers to prevent exponential void growth
    /// 
    /// This supports indefinite insertion up to memory limits.
    /// 
    /// # Complexity
    /// 
    /// O(n) where n is current item count
    /// 
    fn expand(&mut self) {
        // Collect all (canonical, fp, fp_len, is_void) using Iterator
        let entries = self.iterate_entries();
        let old_q_bits = self.quotient_bits;

        // Double the filter
        self.num_slots *= 2;
        self.quotient_bits += 1;
        self.num_expansions += 1;
        self.num_extension_slots += 2;  // Match Java: num_extension_slots += 2
        self.slots = vec![Slot::empty(); self.total_slots()];
        self.metadata = vec![SlotMetadata::new(); self.total_slots()];
        self.num_items = 0;
        self.mother_hashes.clear();

        // Reinsert each entry

        // Track slots that already have a void to prevent exponential void growth
        let mut void_slots: std::collections::HashSet<usize> = std::collections::HashSet::new();

        for (bucket, fingerprint, fp_len, is_void, _old_pos) in entries {
            if is_void {
                // Void: insert to both possible new canonical slots (deduped)
                let s0 = bucket;
                let s1 = bucket | (1 << old_q_bits);
                if s0 < self.num_slots && void_slots.insert(s0) {
                    self.qf_insert_slot(s0, Slot::void_marker());
                    self.num_items += 1;
                }
                if s1 < self.num_slots && void_slots.insert(s1) {
                    self.qf_insert_slot(s1, Slot::void_marker());
                    self.num_items += 1;
                }
            } else if fp_len > 0 {
                // Steal lowest fp bit and use it to extend the slot address
                let pivot = fingerprint & 1;
                let new_bucket = bucket | ((pivot as usize) << old_q_bits);
                let new_fp = fingerprint >> 1;
                let new_len = fp_len - 1;

                if new_bucket < self.num_slots {
                    if new_len == 0 {
                        self.qf_insert_slot(new_bucket, Slot::void_marker());
                    } else {
                        self.qf_insert_slot(new_bucket, Slot::new(new_fp, new_len));
                    }
                    self.num_items += 1;
                }
            }
        }
        return;
    }

    /// Extracts all entries as tuples for expansion.
    /// 
    /// Returns a list of `(canonical, fingerprint, fingerprint_len, is_void, position)` tuples.
    /// Uses the iterator algorithm to track canonical slots while scanning the slot array.
    /// 
    /// The canonical slot is computed by tracking:
    /// - Occupied non-shifted slots (cluster starts)
    /// - Occupied shifted slots and continuations within clusters
    /// - Queue of pending runs
    /// 
    /// # Returns
    /// 
    /// Vec of (canonical_slot, fp_bits, fp_length, is_void, storage_position)
    /// 
    fn iterate_entries(&self) -> Vec<(usize, u64, u8, bool, usize)> {
        let mut result = Vec::new();
        let mut queue: Vec<usize> = Vec::new();
        let mut current_bucket: usize = 0;

        let mut index: usize = 0;
        while index < self.total_slots() {
            // Skip empty slots
            if self.is_slot_empty(index) && self.slots[index].is_empty() {
                index += 1;
                continue;
            }

            let occ = self.metadata[index].is_occupied();
            let cont = self.metadata[index].is_continuation();
            let shift = self.metadata[index].is_shifted();

            if occ && !cont && !shift {
                queue.clear();
                queue.push(index);
                current_bucket = index;
            } else if occ && cont && shift {
                queue.push(index);
            } else if !occ && !cont && shift {
                if !queue.is_empty() { queue.remove(0); }
                current_bucket = queue.first().copied().unwrap_or(index);
            } else if !occ && cont && shift {
                // continuation — bucket unchanged
            } else if occ && !cont && shift {
                queue.push(index);
                if !queue.is_empty() { queue.remove(0); }
                current_bucket = queue.first().copied().unwrap_or(index);
            }

            // Record non-empty data slots
            if !self.slots[index].is_empty() && !self.slots[index].is_tombstone() {
                result.push((
                    current_bucket,
                    self.slots[index].fingerprint(),
                    self.slots[index].length(),
                    self.slots[index].is_void(),
                    index,
                ));
            }

            index += 1;
        }
        return result;
    }
}

// DEBUG

impl std::fmt::Debug for AlephFilter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AlephFilter")
            .field("num_items", &self.num_items)
            .field("num_slots", &self.num_slots)
            .field("num_expansions", &self.num_expansions)
            .field("load_factor", &self.load_factor())
            .finish()
    }
}

