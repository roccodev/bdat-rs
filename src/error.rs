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
}

#[derive(Debug)]
pub enum Scope {
    Table,
    File,
}
