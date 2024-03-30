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
    /// Used in all games prior to XC3
    Legacy(LegacyVersion),
    /// Used in XC3
    Modern,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum LegacyVersion {
    /// Used in XC1 (Wii)
    Wii,
    /// Used in XC2/XCDE
    Switch,
    /// Used in XCX
    X,
}

impl BdatVersion {
    pub fn is_legacy(&self) -> bool {
        *self != BdatVersion::Modern
    }

    pub fn is_modern(&self) -> bool {
        !self.is_legacy()
    }

    /// Gets whether the version forces labels to be hashed.
    pub fn are_labels_hashed(&self) -> bool {
        self.is_modern()
    }

    /// Returns the size in bytes of the table header.
    pub const fn table_header_size(&self) -> usize {
        match self {
            BdatVersion::Modern => 48,
            BdatVersion::Legacy(l) => l.table_header_size(),
        }
    }
}

impl LegacyVersion {
    /// Returns the size in bytes of the table header.
    pub const fn table_header_size(&self) -> usize {
        match self {
            Self::Wii => legacy::HEADER_SIZE_WII,
            _ => legacy::HEADER_SIZE,
        }
    }
}

impl From<LegacyVersion> for BdatVersion {
    fn from(value: LegacyVersion) -> Self {
        Self::Legacy(value)
    }
}
