// This file contains metadata flags for each slot.
// - **Occupied** (bit 0): Canonical slot contains a run
// - **Continuation** (bit 1): Slot continues a run from a previous slot
// - **Shifted** (bit 2): Data was displaced from its canonical position due to collisions
// These flags work together to determine the structure of clusters and runs.

use std::fmt;

#[derive(Clone, Copy, Default, PartialEq, Eq)]
/// A compact metadata tag for one filter slot.
/// 
/// Wraps a single u8 with three bit flags:
/// - Occupied (bit 0)
/// - Continuation (bit 1)
/// - Shifted (bit 2)
pub struct SlotMetadata(u8);

impl SlotMetadata {
    // BIT POSITIONS
    
    /// Bit position for the "occupied" flag.
    /// Indicates this canonical slot is the start of a run.
    const OCCUPIED_BIT: u8 = 0;
    
    /// Bit position for the "continuation" flag.
    /// Indicates this slot continues a run from a previous slot.
    const CONTINUATION_BIT: u8 = 1;
    
    /// Bit position for the "shifted" flag.
    /// Indicates data here was displaced from its canonical position.
    const SHIFTED_BIT: u8 = 2;

    // CONSTRUCTORS

    /// Creates empty metadata (all bits off).
    /// 
    /// # Returns
    /// 
    /// Metadata with all flags cleared
    #[inline]
    pub const fn new() -> Self {
        return Self(0);
    }
    
    /// Creates metadata with specific flag values.
    /// 
    /// # Arguments
    /// 
    /// * `is_occupied` - Whether this canonical slot starts a run
    /// * `is_continuation` - Whether this slot continues a run
    /// * `is_shifted` - Whether this slot's data is displaced from canonical position
    /// 
    /// # Returns
    /// 
    /// Metadata with the specified flags set
    #[inline]
    pub const fn with_flags(is_occupied: bool, is_continuation: bool, is_shifted: bool) -> Self {
        let mut value = 0u8;
        if is_occupied {
            value |= 1 << Self::OCCUPIED_BIT;
        }
        if is_continuation {
            value |= 1 << Self::CONTINUATION_BIT;
        }

        if is_shifted {
            value |= 1 << Self::SHIFTED_BIT;
        }
        return Self(value);
    }

    // QUERIES AND SETTERS

    /// Checks if this metadata is completely empty (all flags off).
    /// 
    /// # Returns
    /// 
    /// `true` if no flags are set
    #[inline]
    pub const fn is_empty(&self) -> bool {
        return self.0 == 0;
    }

    /// Reads the "occupied" flag.
    /// 
    /// Indicates whether this canonical slot is the start of a run.
    /// 
    /// # Returns
    /// 
    /// `true` if occupied
    #[inline]
    pub const fn is_occupied(&self) -> bool {
        return (self.0 >> Self::OCCUPIED_BIT) & 1 == 1;
    }

    /// Sets the "occupied" flag.
    /// 
    /// # Arguments
    /// 
    /// * `value` - `true` to set occupied, `false` to clear
    #[inline]
    pub fn set_occupied(&mut self, value: bool){
        if value {
            self.0 |= 1 << Self::OCCUPIED_BIT;
        } else {
            self.0 &= !(1 << Self::OCCUPIED_BIT);
        }
    }
    
    /// Reads the "continuation" flag.
    /// 
    /// Indicates whether this slot is part of a run that started in a previous slot.
    /// 
    /// # Returns
    /// 
    /// `true` if this is a continuation slot
    #[inline]
    pub const fn is_continuation(&self) -> bool {
        return (self.0 >> Self::CONTINUATION_BIT) & 1 == 1;
    }
    
    /// Sets the "continuation" flag.
    /// 
    /// # Arguments
    /// 
    /// * `value` - `true` to mark as continuation, `false` otherwise
    #[inline]
    pub const fn set_continuation(&mut self, value: bool){
        if value {
            self.0 |= 1 << Self::CONTINUATION_BIT;
        } else{
            self.0 &= !(1 << Self::CONTINUATION_BIT);
        }
    }
    
    /// Reads the "shifted" flag.
    /// 
    /// Indicates whether this slot's data was displaced from its canonical position
    /// due to collisions during insertion.
    /// 
    /// # Returns
    /// 
    /// `true` if this slot is shifted
    #[inline]
    pub const fn is_shifted(&self) -> bool {
        return (self.0 >> Self::SHIFTED_BIT) & 1 == 1;
    }

    /// Sets the "shifted" flag.
    /// 
    /// # Arguments
    /// 
    /// * `value` - `true` to mark as shifted, `false` otherwise
    pub const fn set_shifted(&mut self, value: bool){
        if value{
            self.0 |= 1 << Self::SHIFTED_BIT;
        } else {
            self.0 &= !(1 << Self::SHIFTED_BIT);
        }
    }

    /// Checks if this slot is a cluster start.
    /// 
    /// A cluster start is occupied and not shifted (canonical position).
    /// 
    /// # Returns
    /// 
    /// `true` if this slot starts a cluster
    #[inline]
    pub const fn is_cluster_start(&self) -> bool{
        return self.is_occupied() && !self.is_shifted();
    }

    /// Checks if this slot is a run start within a cluster.
    /// 
    /// A run start is not a continuation and has some presence signal
    /// (either occupied or shifted).
    /// 
    /// # Returns
    /// 
    /// `true` if this slot starts a run
    #[inline]
    pub const fn is_run_start(&self) -> bool {
        return !self.is_continuation() && (self.is_occupied() || self.is_shifted());
    }

    /// Checks if this slot has meaningful metadata.
    /// 
    /// A slot has data if any of the three flags are set.
    /// 
    /// # Returns
    /// 
    /// `true` if any flag is set
    #[inline]
    pub const fn has_data(&self) -> bool {
        return self.is_shifted() || self.is_continuation() || self.is_occupied();
    }
    
    // SERIALIZATION & RAW ACCESS
    
    /// Clears all metadata flags (resets to empty).
    /// 
    /// # Complexity
    /// 
    /// O(1)
    #[inline]
    pub fn clear(&mut self) {
        self.0 = 0
    }

    /// Returns the raw byte value for serialization/storage.
    /// 
    /// # Returns
    /// 
    /// The 8-bit flags value
    pub const fn raw(&self) -> u8 {
        return self.0;
    }

    /// Constructs metadata from a raw byte (for deserialization).
    /// 
    /// # Arguments
    /// 
    /// * `value` - Raw byte containing bit flags
    /// 
    /// # Returns
    /// 
    /// Reconstructed metadata
    pub const fn from_raw(value: u8) -> Self {
        return Self(value);
    }
}

// DEBUG & DISPLAY FORMATTING

impl fmt::Debug for SlotMetadata {
    /// Produces verbose debug output showing all three flags by name.
    /// 
    /// Example: `SlotMetadata { occupied: true, continuation: false, shifted: true }`
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, 
        "SlotMetadata {{ occupied: {}, continuation: {}, shifted: {}}}", 
    self.is_occupied(), 
    self.is_continuation(),
    self.is_shifted()
    )
    }
}

impl fmt::Display for SlotMetadata {
    /// Produces compact 3-character display of metadata flags.
    /// 
    /// Format: `[O|-][C|-][S|-]`
    /// - `O` = occupied, `-` = not occupied
    /// - `C` = continuation, `-` = not continuation
    /// - `S` = shifted, `-` = not shifted
    /// 
    /// Examples: `OCS`, `O--`, `-C-`, `---`
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result{
        write!(f, "{}{}{}", 
        if self.is_occupied() {"O"} else {"-"},
        if self.is_continuation() {"C"} else {"-"},  
        if self.is_shifted() {"S"} else {"-"},
    )
    }
}