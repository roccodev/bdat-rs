pub mod legacy;
pub mod modern;

pub(crate) mod detect;

mod read;

pub use read::BdatFile;

/// Alias for [`byteorder::LittleEndian`], i.e. the byte order used in the Switch games.
pub type SwitchEndian = byteorder::LittleEndian;
/// Alias for [`byteorder::BigEndian`], i.e. the byte order used in the Wii/Wii U games.
pub type WiiEndian = byteorder::BigEndian;

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum BdatVersion {
    /// Used in XC1/XC2/XCDE
    Legacy,
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
}
