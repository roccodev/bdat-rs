use crate::table::convert::FormatConvertError;
use crate::{BdatVersion, DetectError, ValueType};
use std::num::TryFromIntError;
use std::str::Utf8Error;
use thiserror::Error;

/// Alias for `Result<T, BdatError>`
pub type Result<T> = std::result::Result<T, BdatError>;

/// Errors that may occur while reading and writing BDAT tables
#[derive(Error, Debug)]
pub enum BdatError {
    #[error(transparent)]
    Utf8(#[from] Utf8Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("Malformed BDAT ({0:?})")]
    MalformedBdat(Scope),
    #[error(transparent)]
    InvalidLength(#[from] TryFromIntError),
    #[error("Unknown cell type: {0}")]
    UnknownCellType(u8),
    #[error("Unknown value type: {0}")]
    UnknownValueType(u8),
    #[error("Unsupported type: BDAT version {1:?} does not support value type {0:?}")]
    UnsupportedType(ValueType, BdatVersion),
    #[error("Invalid flag type: value type {0:?} does not support flags")]
    InvalidFlagType(ValueType),
    #[error("Could not detect version: {0}")]
    VersionDetect(#[from] DetectError),
    #[error("Could not convert table: {0}")]
    FormatConvert(#[from] FormatConvertError),
    #[error("Unsupported cast type for {0:?}")]
    ValueCast(ValueType),
    #[error("Name table contains duplicate ID <{0:08X}>")]
    NameTableDuplicate(u32),
    #[error("Name table contains ID pair (<{0:08X}>, <{1:08X}>) in incorrect order")]
    NameTableOrder(u32, u32),
}

#[derive(Debug)]
pub enum Scope {
    Table,
    File,
}
