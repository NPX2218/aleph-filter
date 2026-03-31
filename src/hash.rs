// Provides hashing utilities to split a hash value into quotient (slot index)
// and remainder (fingerprint) components for the quotient filter.

use xxhash_rust::xxh3::xxh3_64;

/// Generates a 64-bit hash from the given data using xxHash3.
///
/// xxHash3 is a fast, non-cryptographic hash function suitable for
/// filter applications.
/// 
/// # Arguments
///
/// * `data` - Byte slice to hash
/// 
/// # Returns
/// 
/// 64-bit hash value
/// 
/// # Complexity
/// 
/// O(n) where n is length of data
#[inline]
pub fn hash_key(data: &[u8]) -> u64 {
    return xxh3_64(data);
}

/// Splits a hash into quotient (slot index) and remainder (fingerprint) components.
/// 
/// Given a 64-bit hash, extracts:
/// - Lower `q_bits` bits as quotient (canonical slot address)
/// - Next `r_bits` bits as remainder (fingerprint to store)
/// 
/// # Arguments
///
/// * `hash` - Input 64-bit hash value
/// * `q_bits` - Number of bits to use for quotient (typically log2(num_slots))
/// * `r_bits` - Number of bits to use for remainder (fingerprint width)
/// 
/// # Returns
/// 
/// Tuple of `(quotient, remainder)`
/// 
/// # Example
/// 
/// ```
/// use aleph_filter::hash::split_hash;
/// let hash = 0x123456789ABCDEF0u64;
/// let (q, r) = split_hash(hash, 16, 32);  // 16-bit quotient, 32-bit remainder
/// ```
#[inline]
pub fn split_hash(hash: u64, q_bits: u32, r_bits: u32) -> (u64, u64) {
    let q_mask = if q_bits >= 64 {u64::MAX} else {(1u64 << q_bits) - 1};  
    let quotient = hash & q_mask;

    let r_mask = if r_bits >= 64 { u64::MAX } else {(1u64 << r_bits) - 1};
    let remainder = (hash >> q_bits) & r_mask;

    return (quotient, remainder);
}

/// Combines quotient and remainder back into a single hash value.
/// 
/// Inverse of `split_hash()`. Places quotient in lower bits,
/// remainder in upper bits.
/// 
/// # Arguments
///
/// * `quotient` - Slot index (lower bits)
/// * `remainder` - Fingerprint value (upper bits)
/// * `q_bits` - Number of quotient bits (for alignment)
/// 
/// # Returns
/// 
/// Combined 64-bit hash value
/// 
#[inline]
pub fn combine_hash(quotient: u64, remainder: u64, q_bits: u32) -> u64 {
    return (remainder << q_bits) | quotient;
}

/// Calculates the number of quotient bits needed for a given slot count.
/// 
/// Computes ceil(log2(num_slots)), which is the minimum number of bits
/// needed to address all slots.
/// 
/// # Arguments
///
/// * `num_slots` - Number of slots in the filter
/// 
/// # Returns
/// 
/// Number of quotion bits (log2 of num_slots, rounded up)
/// 
/// # Example
/// 
/// ```
/// use aleph_filter::hash::quotient_bits_for_slots;
/// assert_eq!(quotient_bits_for_slots(8), 3);   // 8 = 2^3
/// assert_eq!(quotient_bits_for_slots(10), 4);  // 10 needs 4 bits
/// ```
#[inline]
pub fn quotient_bits_for_slots(num_slots: usize) -> u32{
    if num_slots <= 1 {
        return 1;
    }

    // Compute ceil(log2(num_slots))
    return (usize::BITS - (num_slots - 1).leading_zeros()) as u32;
}

/// Calculates the fingerprint width (in bits) required to achieve a target FPR.
/// 
/// Uses the relationship: `FPR = 2^(-fingerprint_bits)`
/// Therefore: `fingerprint_bits = ceil(-log2(FPR))`
/// 
/// # Arguments
///
/// * `fpr` - Desired false positive rate (e.g., 0.01 for 1%)
/// 
/// # Returns
/// 
/// Number of fingerprint bits needed
/// 
/// # Panics
/// 
/// If `fpr` is not in the range (0, 1).
/// 
/// # Example
/// 
/// ```
/// use aleph_filter::hash::fingerprint_bits_for_fpr;
/// assert_eq!(fingerprint_bits_for_fpr(0.01), 7);   // 1% FPR needs ~7 bits
/// assert_eq!(fingerprint_bits_for_fpr(0.001), 10); // 0.1% FPR needs ~10 bits
/// ```
#[inline]
pub fn fingerprint_bits_for_fpr(fpr: f64) -> u32 {
    // FPR = 1/2^f = 2^(-f), hence f = -log2(FPR)
    assert!(fpr > 0.0 && fpr < 1.0, "FPR must be between 0 and 1.");
    return (-fpr.log2()).ceil() as u32;
}

