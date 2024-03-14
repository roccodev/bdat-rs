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
pub use legacy::{LegacyRow, LegacyTable};
pub use modern::{ModernRow, ModernTable};

/// A BDAT table. Depending on how they were read, BDAT tables can either own their data source
/// or borrow from it.
///
/// ## Accessing cells
/// The [`Table::row`] function provides an easy interface to access cells.
///
/// See also: [`RowRef`]
///
/// ## Specialized views
/// If you know what type of BDAT tables you're dealing with (legacy or modern), you can use
/// [`as_modern`] and [`as_legacy`] to get specialized table views.
///
/// These views return more ergonomic row accessors that let you quickly extract values, instead
/// of having to handle cases that are not supported by the known version.
///
/// See also: [`ModernTable`], [`LegacyTable`]
///
/// ## Adding/deleting rows
/// The table's mutable iterator does not allow structural modifications to the table. To add or
/// delete rows, re-build the table. (`CompatTableBuilder::from(table)`)
///
/// ## Examples
///
/// ```
/// use bdat::{Table, CompatTableBuilder, Cell, ColumnDef, Value, ValueType, Label, BdatVersion};
///
/// let table: Table = CompatTableBuilder::with_name(Label::Hash(0xDEADBEEF))
///     .set_base_id(1) // default, if you want 0 it must be set manually
///     .add_column(ColumnDef::new(ValueType::UnsignedInt, Label::Hash(0xCAFEBABE)))
///     .add_row(vec![Cell::Single(Value::UnsignedInt(10))].into())
///     .build(BdatVersion::Modern);
///
/// assert_eq!(table.row_count(), 1);
/// assert_eq!(
///     *table.row(1).get(Label::Hash(0xCAFEBABE)).as_single().unwrap(),
///     Value::UnsignedInt(10)
/// );
/// ```
///
/// [`as_legacy`]: Table::as_legacy
/// [`as_modern`]: Table::as_modern
#[derive(Debug, Clone, PartialEq)]
pub struct Table<'b> {
    inner: TableInner<'b>,
}

#[derive(Debug, Clone, PartialEq)]
enum TableInner<'b> {
    Modern(ModernTable<'b>),
    Legacy(LegacyTable<'b>),
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
