// A `Slot` packs fingerprint data and its length into a single 64-bit integer:
//   - Lower 56 bits: fingerprint value (up to 56 bits)
//   - Upper 8 bits: fingerprint length in bits (how many of the 56 are valid)
//
// Special markers:
//   - VOID_MARKER: represents an exhausted fingerprint (length = 0)
//   - TOMBSTONE_MARKER: marks deleted void entries

#[derive(Clone, Copy, Default, PartialEq, Eq)]
/// A 64-bit packed slot storing fingerprint + length.
/// 
/// Layout:
/// - Bits \[0:55\]: fingerprint value (56 bits max)
/// - Bits \[56:63\]: fingerprint length in bits (8 bits)
pub struct Slot {
    /// Combined fingerprint and length in a single 64-bit integer.
    encoded: u64,
}

impl Slot {
    
    /// Bit position where the length field starts (upper 8 bits).
    const LENGTH_SHIFT: u32 = 56;

    /// Mask for extracting fingerprint (lower 56 bits).
    const FP_MASK: u64 = (1u64 << Self::LENGTH_SHIFT) - 1;
    
    /// Special marker for void entries (exhausted fingerprints).
    /// Set when fingerprint bits are sacrificed during expansion.
    const VOID_MARKER: u64 = 0x8000_0000_0000_0000;
    
    /// Special marker for tombstones (deleted void entries).
    /// Used during deletion to avoid costly compaction.
    const TOMBSTONE_MARKER: u64 = 0xFFFF_FFFF_FFFF_FFFE;
    
    /// Creates a slot with the given fingerprint and length.
    /// 
    /// Packs the fingerprint into lower 56 bits and length into upper 8 bits.
    /// 
    /// # Arguments
    /// 
    /// * `fingerprint` - Bit pattern to store (automatically masked to 56 bits)
    /// * `length` - How many bits of fingerprint are actually valid (0-56)
    /// 
    /// # Panics
    /// 
    /// In debug mode if `length > 56`
    /// 
    /// # Example
    /// 
    /// ```
    /// use aleph_filter::slot::Slot;
    /// let slot = Slot::new(0x123456, 24);  // Store 24-bit fingerprint
    /// ```
    #[inline]
    pub fn new(fingerprint : u64, length: u8) -> Self {
        debug_assert!(length <= 56, "Fingerprint length cannot exceed 56 bits");
        return Self {
                // Shift length into upper 8 bits, mask fingerprint to lower 56
                encoded: ((length as u64) << Self::LENGTH_SHIFT) | (fingerprint & Self::FP_MASK),
        };
    }
    
    /// Creates an empty slot with all bits zero.
    /// 
    /// An empty slot indicates no data stored at this position.
    /// 
    /// # Returns
    /// 
    /// Slot with `encoded = 0`
    #[inline]
    pub const fn empty() -> Self {
        return Self { encoded: 0 };
    }

    /// Creates a void marker slot.
    /// 
    /// Void markers are used when fingerprint bits are exhausted during expansion,
    /// or when inserting entries that hash to the same canonical bucket.
    /// They still match any query (indicating "something is here, but bits are gone").
    /// 
    /// # Returns
    /// 
    /// Slot with VOID_MARKER value
    #[inline]
    pub const fn void_marker() -> Self {
        return Self {
            encoded: Self::VOID_MARKER,
        };
    }

    /// Creates a tombstone marker slot.
    /// 
    /// Tombstones are used during deletion of void entries to avoid compaction overhead.
    /// 
    /// # Returns
    /// 
    /// Slot with TOMBSTONE_MARKER value
    #[inline]
    pub const fn tombstone() -> Self {
        return Self {
            encoded: Self::TOMBSTONE_MARKER,
        }
    }

    // QUERIES

    /// Returns the fingerprint bits (lower 56 bits, stripped of length).
    /// 
    /// # Returns
    /// 
    /// Fingerprint value
    #[inline]
    pub const fn fingerprint(&self) -> u64 {
        return self.encoded & Self::FP_MASK;
    }

    /// Returns the fingerprint length in bits.
    /// 
    /// This indicates how many of the 56 available fingerprint bits are actually valid.
    /// 
    /// # Returns
    /// 
    /// Length in bits (0-56)
    #[inline]
    pub const fn length(&self) -> u8 {
        return (self.encoded >> Self::LENGTH_SHIFT) as u8;
    }

    /// Checks if this slot is completely empty (no data, no markers).
    /// 
    /// # Returns
    /// 
    /// `true` if encoded value is zero
    #[inline]
    pub const fn is_empty(&self) -> bool {
        return self.encoded == 0;
    }

    /// Checks if this slot is a void marker.
    /// 
    /// Void means the fingerprint ID exhausted (length = 0) but something is still stored here.
    /// This particularly happens after expansion when bits are sacrificed.
    /// 
    /// # Returns
    /// 
    /// `true` if void marker or length is 0 but not empty
    #[inline]
    pub const fn is_void(&self) -> bool {
        return self.encoded == Self::VOID_MARKER || (self.length() == 0 && !self.is_empty());
    }

    /// Checks if this slot is a tombstone marker.
    /// 
    /// Tombstones mark deleted void entries and avoid costly run compaction.
    /// 
    /// # Returns
    /// 
    /// `true` if tombstone marker
    #[inline]
    pub const fn is_tombstone(&self) -> bool {
        return self.encoded == Self::TOMBSTONE_MARKER;
    }

    /// Checks if this slot contains actual fingerprint data.
    /// 
    /// Returns `true` only for slots that have real data (not empty, void, or tombstone).
    /// 
    /// # Returns
    /// 
    /// `true` if valid fingerprint is present
    #[inline]
    pub const fn has_fingerprint(&self) -> bool {
        return !self.is_empty() && !self.is_void() && !self.is_tombstone();
    }

    // RAW ACCESS & SERIALIZATION

    /// Returns the entire raw 64-bit encoded value.
    /// 
    /// Useful for serialization or direct inspection.
    /// 
    /// # Returns
    /// 
    /// Raw 64-bit value
    #[inline]
    pub const fn raw(&self) -> u64 {
        return self.encoded;
    }

    /// Reconstructs a slot from a raw 64-bit value.
    /// 
    /// Inverse of `raw()`. Used for deserialization.
    /// 
    /// # Arguments
    /// 
    /// * `encoded` - Raw 64-bit value
    /// 
    /// # Returns
    /// 
    /// Reconstructed slot
    #[inline]
    pub const fn from_raw(encoded: u64) -> Self {
        return Self { encoded };
    }
    
    // EXPANSION SUPPORT
    
    /// Steals the rightmost fingerprint bit during expansion.
    /// 
    /// Used when expanding the filter:
    /// 1. Extracts the lowest (rightmost) bit from the fingerprint
    /// 2. Shifts fingerprint right by 1 bit
    /// 3. Decreases length by 1
    /// 4. Converts to void marker if length reaches 0
    /// 
    /// This stolen bit becomes part of the new quotient to address the expanded filter.
    /// 
    /// # Returns
    /// 
    /// `Some(bit)` where bit is 0 or 1, or `None` if slot cannot provide a bit
    /// (empty, void, tombstone, or already length=0)
    /// 
    /// # Example
    /// 
    /// Slot with fingerprint `0b1011` (length 4) → steal bit 1, becomes `0b101` (length 3)
    #[inline]
    pub fn steal_bit(&mut self) -> Option<u64> {

        if self.is_empty() || self.is_void() || self.is_tombstone(){
            return None;
        }
        
        let fp = self.fingerprint();
        let len = self.length();

        if len == 0 {
            return None;
        }

        // Extract the lowest fingerprint bit (used as a quotient expansion bit)
        let stolen_bit = fp & 1;
        // Shift fingerprint right to remove the stolen bit
        let new_fp = fp >> 1;
        // Decrease the effective fingerprint width
        let new_len = len - 1;
        
        if new_len == 0 {
            // No bits left: convert to void marker
            *self = Slot::void_marker();
        } else {
            // Update with decreased fingerprint
            *self = Slot::new(new_fp, new_len);
        }
        return Some(stolen_bit);
    }

    // MATCHING (QUERY SUPPORT)

    /// Checks if this stored slot matches a query fingerprint.
    /// 
    /// Matching rules:
    /// - Tombstones and empty slots never match
    /// - Void markers always match (something is here, we lost the bits)
    /// - Otherwise, compares the minimum overlap of stored and query fingerprint bits
    /// 
    /// # Arguments
    /// 
    /// * `query_fp` - Fingerprint to match against
    /// * `query_len` - Length of query fingerprint in bits
    /// 
    /// # Returns
    /// 
    /// `true` if slot could contain this fingerprint
    /// 
    /// # Example
    /// 
    /// Stored: `0b110` (length 3), Query: `0b1101` (length 4)
    /// Compare: min(3, 4) = 3 bits → `0b110` == `0b110` means match
    #[inline]
    pub fn matches(&self, query_fp: u64, query_len: u8) -> bool {
        if self.is_tombstone() || self.is_empty() {
            return false;
        }

        if self.is_void() {
            return true;
        }

        let stored_len = self.length();
        let stored_fp = self.fingerprint();

        // Compare only the bits both have available
        let compare_len = stored_len.min(query_len);
        if compare_len == 0 {
            return true;  // Both have no bits, consider it a match
        }

        // Create mask for the bits we're comparing
        let mask = (1u64 << compare_len) - 1;
        return (stored_fp & mask) == (query_fp & mask);
    }
}



