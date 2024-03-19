//! BDAT table, row, cell implementations

use crate::{RowId, ValueType};
use thiserror::Error;

pub mod cell;
pub mod column;
pub mod compat;
pub mod row;

mod builder;
mod legacy;
mod modern;
mod util;

pub use builder::{CompatTableBuilder, LegacyTableBuilder, ModernTableBuilder};
pub use column::{LegacyColumn, ModernColumn};
pub use compat::{CompatColumn, CompatRef, CompatRow, CompatTable};
pub use legacy::{LegacyRow, LegacyTable};
pub use modern::{ModernRow, ModernTable};

pub trait Table<'buf> {
    type Id: From<u8>;
    type Name;
    type Row;
    type BuilderRow;
    type Column: crate::Column;
    type BuilderColumn: crate::Column;
}

/// Error encountered while converting tables
/// to a different format.
#[derive(Error, Debug)]
pub enum FormatConvertError {
    /// One of the table's columns has an unsupported value type.
    ///
    /// For example, legacy tables do not support hash-ref fields.
    #[error("unsupported value type {0:?}")]
    UnsupportedValueType(ValueType),
    /// One of the table's values has an unsupported cell type.
    ///
    /// For instance, modern tables only support single-value cells.
    #[error("unsupported cell")]
    UnsupportedCell,
    /// The max number of rows in the table has been reached, so no
    /// more rows can be added.
    #[error("max row count exceeded")]
    MaxRowCountExceeded,
    /// The destination format (legacy) does not support the row ID because it is too high.
    /// This can happen if the base ID or any of the rows's ID is outside of the format's
    /// row ID boundaries.
    #[error("unsupported row ID {0}")]
    UnsupportedRowId(RowId),
    /// The destination format does not support hashed labels.
    #[error("unsupported label type")]
    UnsupportedLabelType,
}
