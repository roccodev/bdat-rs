use crate::{BdatVersion, DetectError, ValueType};
use std::num::TryFromIntError;
use std::str::Utf8Error;
use thiserror::Error;

pub type Result<T> = std::result::Result<T, BdatError>;

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
    #[error("Unsupported type: BDAT version {0:?} does not support value type {1:?}")]
    UnsupportedType(ValueType, BdatVersion),
    #[error("Invalid flag type: value type {0:?} does not support flags")]
    InvalidFlagType(ValueType),
    #[error("Could not detect version: {0}")]
    VersionDetect(#[from] DetectError),
}

#[derive(Debug)]
pub enum Scope {
    Table,
    File,
}
