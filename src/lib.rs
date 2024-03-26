//! # bdat-rs
//!
//! BDAT is a proprietary binary file format used by [MONOLITHSOFT] for their Xenoblade Chronicles
//! games. It is a tabular data format (like CSV, TSV, etc.) with named tables and typed fields.  
//! In newer versions of the format, files and tables (also called sheets) can be memory-mapped
//! for use in programs based on the C memory model.
//!
//! This crate allows reading and writing BDAT tables and files.
//!
//! ## Reading BDAT tables
//!
//! ### Reading Xenoblade 3 ("modern") tables
//!
//! The crate exposes the [`from_bytes`] and [`from_reader`] functions in the [`modern`] module
//! to parse Xenoblade 3 BDAT files from a slice or a [`std::io::Read`] stream respectively.
//!
//! The [`label_hash!`] macro can be used to quickly generate hashed labels from plain-text strings.
//!
//! See also: [`ModernTable`]
//!
//! ```
//! use bdat::{BdatResult, SwitchEndian, BdatFile, ModernTable, label_hash};
//! use bdat::hash::murmur3_str;
//!
//! fn read_xc3() -> BdatResult<()> {
//!     let data = [0u8; 0];
//!     // also bdat::from_reader for io::Read implementations.
//!     let mut bdat_file = bdat::modern::from_bytes::<SwitchEndian>(&data)?;
//!
//!     let table: &ModernTable = &bdat_file.get_tables()?[0];
//!     if table.name() == &label_hash!("CHR_PC") {
//!         // Found the character table, get Noah's HP at level 99
//!         let noah = table.row(1);
//!         // Alternatively, if the `hash-table` feature is enabled (default)
//!         let noah = table.row_by_hash(murmur3_str("PC_NOAH"));
//!
//!         let noah_hp = noah
//!             .get(label_hash!("HpMaxLv99"))
//!             .get_as::<u32>();
//!     }
//!
//!     Ok(())
//! }
//! ```
//!
//! ### Reading tables from other games ("legacy")
//!
//! Similarly, the [`legacy`] module contains functions to read and write legacy tables.
//!
//! There are differences between games that use the legacy format, so specifying a
//! [`BdatVersion`] is required.  
//! If you don't know the legacy sub-version, you can use [`detect_file_version`] or
//! [`detect_bytes_version`].
//!
//! See also: [`LegacyTable`]
//!
//! ```
//! use bdat::{BdatResult, SwitchEndian, BdatFile, LegacyTable, BdatVersion, Label};
//!
//! fn read_legacy() -> BdatResult<()> {
//!     // Mutable access is required as text might need to be unscrambled.
//!     let mut data = [0u8; 0];
//!     // Use `WiiEndian` for Xenoblade (Wii) and Xenoblade X.
//!     let mut bdat_file = bdat::legacy::from_bytes::<SwitchEndian>(
//!         &mut data,
//!         BdatVersion::LegacySwitch
//!     )?;
//!
//!     let table: &LegacyTable = &bdat_file.get_tables()?[0];
//!     if table.name() == "CHR_Dr" {
//!         // Found the character table, get Rex's HP at level 99
//!         let rex = table.row(1);
//!         // We need to distinguish between legacy cell types
//!         let rex_hp = rex.get("HpMaxLv99")
//!             .as_single()
//!             .unwrap()
//!             .get_as::<u32>();
//!     }
//!
//!     Ok(())
//! }
//! ```
//!
//! ### Version auto-detect
//!
//! If the table format isn't known, [`from_bytes`] and [`from_reader`] from the
//! crate root can be used instead.
//!
//! ```
//! use bdat::{BdatResult, SwitchEndian, BdatFile, CompatTable, Label, label_hash};
//!
//! fn read_detect() -> BdatResult<()> {
//!     // Mutable access is required, as this might be a legacy table.
//!     let mut data = [0u8; 0];
//!     // Endianness is also detected automatically.
//!     let mut bdat_file = bdat::from_bytes(&mut data)?;
//!
//!     // Can no longer assume the format.
//!     let table: &CompatTable = &bdat_file.get_tables()?[0];
//!     if table.name() == label_hash!("CHR_PC") {
//!         // Found the character table, get Noah's HP at level 99.
//!         // No hash lookup for rows!
//!         let noah = table.row(1);
//!         // We can't use the ergonomic functions from `ModernTable` here,
//!         // so we need to handle the legacy cases, even if they don't
//!         // concern modern tables.
//!         let noah_hp = noah.get(label_hash!("HpMaxLv99"))
//!             .as_single()
//!             .unwrap()
//!             .get_as::<u32>();
//!     }
//!
//!     Ok(())
//! }
//! ```
//!
//! ## Writing BDAT tables
//! The `to_vec` and `to_writer` functions (in [`legacy`] and [`modern`]) can be used to write BDAT
//! files to a vector or a [`std::io::Write`] implementation.
//!
//! Writing fully requires the user to specify the BDAT version to use, by choosing the
//! appropriate module implementation.
//!
//! Tables obtained with the auto-detecting functions must be extracted or converted first.
//!
//! ```
//! use bdat::{BdatResult, BdatVersion, SwitchEndian, WiiEndian, ModernTable, LegacyTable};
//!
//! fn write_modern(table: &ModernTable) -> BdatResult<()> {
//!     // also bdat::to_writer for io::Write implementations
//!     let _written: Vec<u8> = bdat::modern::to_vec::<SwitchEndian>([table])?;
//!     Ok(())
//! }
//!
//! fn write_legacy(table: &LegacyTable) -> BdatResult<()> {
//!     // Endianness and version may vary, here it's writing Xenoblade X tables.
//!     let _written: Vec<u8> = bdat::legacy::to_vec::<WiiEndian>([table], BdatVersion::LegacyX)?;
//!     Ok(())
//! }
//! ```
//!
//! ## Serde support
//! When the `serde` feature flag is enabled, this crate's types will implement `Serialize` and
//! `Deserialize`.
//!
//! While the crate doesn't support serializing/deserializing BDAT to Rust types, this can be used
//! to transcode BDAT to other formats.  
//! The [bdat-toolset] crate will convert BDAT to CSV and JSON, and JSON to BDAT.
//!
//! [MONOLITHSOFT]: https://www.monolithsoft.co.jp/
//! [bdat-toolset]: https://github.com/RoccoDev/bdat-rs/tree/master/toolset

pub mod hash;
#[cfg(feature = "serde")]
pub mod serde;

pub(crate) mod error;
pub(crate) mod io;
pub mod label;
pub mod table;

pub use error::BdatError;
pub use error::Result as BdatResult;
pub use io::detect::*;
pub use io::*;
pub use label::Label;

pub use table::cell::*;
pub use table::column::*;
pub use table::compat::*;
pub use table::row::*;
pub use table::{
    CompatTable, LegacyColumn, LegacyRow, LegacyTable, LegacyTableBuilder, ModernColumn, ModernRow,
    ModernTable, ModernTableBuilder,
};
