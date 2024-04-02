pub mod legacy;
pub mod modern;

pub(crate) mod detect;

mod read;

pub use read::BdatFile;

const BDAT_MAGIC: [u8; 4] = [b'B', b'D', b'A', b'T'];

/// Alias for [`byteorder::LittleEndian`], i.e. the byte order used in Xenoblade 3D and
/// in the Switch games.
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
    /// Used in XC3D (New 3DS)
    New3ds,
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
}

impl LegacyVersion {
    /// Returns the size in bytes of the table header.
    pub(crate) const fn table_header_size(&self) -> usize {
        if self.is_wii_table_format() {
            legacy::HEADER_SIZE_WII
        } else {
            legacy::HEADER_SIZE
        }
    }

    pub(crate) const fn is_wii_table_format(&self) -> bool {
        matches!(self, Self::Wii | Self::New3ds)
    }
}

impl From<LegacyVersion> for BdatVersion {
    fn from(value: LegacyVersion) -> Self {
        Self::Legacy(value)
    }
}
