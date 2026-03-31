//! # Aleph Filter 
//!
//! ## Overview
//!
//! The Aleph Filter is a probabilistic data structure for approximate membership queries with:
//! - **O(1) expected insertion and lookup time**
//! - **Tunable false positive rate (FPR)** without cryptographic hashing
//! - **Automatic expansion** with fingerprint bit sacrifice
//! - **Deletion support** via tombstone markers
//! - **Compact memory usage** (~1.3-2 bits per item for typical FPR ranges)
//!
//! ## Quick Start
//!
//! ```
//! use aleph_filter::AlephFilter;
//!
//! let mut filter = AlephFilter::new(1000, 0.01); // 1000 items, 1% FPR
//! filter.insert(b"hello");
//! assert!(filter.contains(b"hello"));
//! filter.delete(b"hello");
//! ```
//!
//! ## Architecture
//!
//! - **`hash`**: Hash splitting into quotient (slot index) and remainder (fingerprint)
//! - **`slot`**: 64-bit packing of fingerprint + length, special markers (void, tombstone)
//! - **`metadata`**: Per-slot flags (occupied, continuation, shifted) for cluster/run tracking
//! - **`aleph_filter`**: Main quotient filter with insertion, lookup, deletion, and expansion
//!
//! ## Key Concepts
//!
//! ### Quotient Filter Terminology
//! - **Canonical slot**: Hash-determined target position for a key
//! - **Run**: Sequence of slots sharing the same canonical slot
//! - **Cluster**: Sequence of consecutive occupied or shifted slots
//! - **Quotient**: Low-order hash bits indicating canonical slot
//! - **Remainder (Fingerprint)**: High-order hash bits stored as compact fingerprint
//!
//! ### Expansion
//! When load factor exceeds threshold (0.9):
//! 1. Double the slot count
//! 2. Increment quotient width by 1 bit
//! 3. Sacrifice 1 bit per entry from fingerprints
//! 4. Re-insert using extracted bit to determine new canonical slots
//!
//! ## Files
//!
//! - `hash.rs` - Hash functions and utilities
//! - `slot.rs` - Compact 64-bit slot storage
//! - `metadata.rs` - Per-slot metadata flags
//! - `aleph_filter.rs` - Main filter implementation

pub mod hash;
pub mod metadata;
pub mod slot;
mod aleph_filter; // private: users can't access the module directly

// pub mod aleph_filter;
// pub use aleph_filter::AlephFilter;

pub use aleph_filter::AlephFilter;

pub mod prelude {
    pub use crate::AlephFilter;
}
