pub mod legacy;
pub mod modern;

pub(crate) mod detect;

mod read;

pub use read::BdatFile;

const BDAT_MAGIC: [u8; 4] = [b'B', b'D', b'A', b'T'];

/// Alias for [`byteorder::LittleEndian`], i.e. the byte order used in the Switch games.
pub type SwitchEndian = byteorder::LittleEndian;
/// Alias for [`byteorder::BigEndian`], i.e. the byte order used in the Wii/Wii U games.
pub type WiiEndian = byteorder::BigEndian;

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum BdatVersion {
    /// Used in XC1 (Wii)
    LegacyWii,
    /// Used in XC2/XCDE
    LegacySwitch,
    /// Used in XCX
    LegacyX,
    /// Used in XC3
    Modern,
}

impl BdatVersion {
    /// Gets whether the version forces labels to be hashed.
    pub fn are_labels_hashed(&self) -> bool {
        *self == BdatVersion::Modern
    }

    /// Returns the size in bytes of the table header.
    pub const fn table_header_size(&self) -> usize {
        match self {
            BdatVersion::Modern => 48,
            BdatVersion::LegacyWii => legacy::HEADER_SIZE_WII,
            _ => legacy::HEADER_SIZE,
        }
    }
}
