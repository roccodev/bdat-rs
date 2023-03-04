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
//! The crate exposes the [`from_bytes`] and [`from_reader`] functions to parse BDAT files from
//! a slice or a [`std::io::Read`] stream respectively.
//!
//! For I/O operations, the byte order must also be specified. For BDAT files used by the Switch
//! games (DE, 2, 3), use [`SwitchEndian`]. For the original Xenoblade Chronicles and X, use
//! [`WiiEndian`].
//! ```
//! use bdat::{BdatResult, SwitchEndian};
//!
//! fn read_tables() -> BdatResult<()> {
//!     let data = [0u8; 0];
//!     // also bdat::from_reader for io::Read implementations
//!     let mut bdat_file = bdat::from_bytes::<SwitchEndian>(&data)?;
//!     let table = &bdat_file.get_tables()?[0];
//!     Ok(())
//! }
//! ```
//!
//! ## Writing BDAT tables
//! The [`to_vec`] and [`to_writer`] functions can be used to write BDAT files to a vector or a
//! [`std::io::Write`] implementation.
//!
//! Unlike reading (where it is detected automatically), writing also requires the user to specify
//! the BDAT version to use.
//! ```
//! use bdat::{BdatResult, BdatVersion, RawTable, SwitchEndian};
//!
//! fn write_table(table: &RawTable) -> BdatResult<()> {
//!     // also bdat::to_writer for io::Write implementations
//!     let _written: Vec<u8> = bdat::to_vec::<SwitchEndian>(BdatVersion::Modern, [table])?;
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

pub mod error;
pub mod hash;
pub mod io;
#[cfg(feature = "serde")]
mod serde;
pub mod types;

pub use error::BdatError;
pub use error::Result as BdatResult;
pub use io::*;
pub use types::*;
