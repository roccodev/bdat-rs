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
//! When reading, if the format version is known, it's better to use version-specific functions.
//! (e.g. [`legacy::from_reader`] and [`modern::from_reader`])
//!
//! ```
//! use bdat::{BdatResult, SwitchEndian, BdatFile};
//!
//! fn read_tables() -> BdatResult<()> {
//!     let mut data = [0u8; 0];
//!     // also bdat::from_reader for io::Read implementations. Additionally,
//!     // by using `bdat::from_bytes` (which automatically detects the version),
//!     // we need mutable access to the data, as we might potentially have to
//!     // unscramble text in legacy formats.
//!     let mut bdat_file = bdat::from_bytes(&mut data)?;
//!     let table = &bdat_file.get_tables()?[0];
//!     Ok(())
//! }
//! ```
//!
//! ## Writing BDAT tables
//! The `to_vec` and `to_writer` functions can be used to write BDAT files to a vector or a
//! [`std::io::Write`] implementation.
//!
//! Unlike reading (where it is detected automatically), writing also requires the user to specify
//! the BDAT version to use, by choosing the appropriate module implementation.
//! ```
//! use bdat::{BdatResult, BdatVersion, Table, SwitchEndian};
//!
//! fn write_table(table: &Table) -> BdatResult<()> {
//!     // also bdat::to_writer for io::Write implementations
//!     let _written: Vec<u8> = bdat::modern::to_vec::<SwitchEndian>([table])?;
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
pub(crate) mod label;
pub(crate) mod table;

pub use error::BdatError;
pub use error::Result as BdatResult;
pub use io::detect::*;
pub use io::*;
pub use label::*;

pub use table::cell::*;
pub use table::column::*;
pub use table::row::*;
pub use table::*;
