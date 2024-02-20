//! BDAT table, row, cell implementations

use crate::{CellAccessor, Label, RowRef, ValueType};
use thiserror::Error;

pub mod cell;
pub mod column;
pub mod compat;
pub mod row;

mod builder;
mod legacy;
mod modern;
mod util;

pub use builder::{LegacyTableBuilder, ModernTableBuilder, TableBuilder};
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
/// delete rows, re-build the table. (`TableBuilder::from(table)`)
///
/// ## Examples
///
/// ```
/// use bdat::{Table, TableBuilder, Cell, ColumnDef, Row, Value, ValueType, Label, BdatVersion, TableAccessor};
///
/// let table: Table = TableBuilder::with_name(Label::Hash(0xDEADBEEF))
///     .add_column(ColumnDef::new(ValueType::UnsignedInt, Label::Hash(0xCAFEBABE)))
///     .add_row(Row::new(1, vec![Cell::Single(Value::UnsignedInt(10))]))
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
}

/// Provides common functions to access rows and columns from a table.
///
/// ## Future compatibility
///
/// Starting from Rust 1.75.0 (#91611) and (tentatively) bdat-rs 0.5.0, this trait may feature
/// iterators to access rows and columns. Those iterators will replace the associated
/// functions in the implementors of this trait.
pub trait TableAccessor<'t, 'b: 't> {
    /// The returned row view type
    type Row: CellAccessor;
    type RowMut: CellAccessor;
    /// The integer type that defines the boundaries of a row ID
    type RowId;

    /// Returns the table's name.
    fn name(&self) -> &Label;

    /// Updates the table's name.
    fn set_name(&mut self, name: Label);

    /// Gets the minimum row ID in the table.
    fn base_id(&self) -> Self::RowId;

    /// Gets a row by its ID.
    ///
    /// ## Panics
    /// If there is no row for the given ID.
    fn row(&'t self, id: Self::RowId) -> RowRef<'t, Self::Row> {
        self.get_row(id).expect("row not found")
    }

    /// Gets a mutable view of a row by its ID
    ///
    /// Note: the ID is the row's numerical ID, which could be different
    /// from the index of the row in the table's row list. That is because
    /// BDAT tables can have arbitrary start IDs.
    ///
    /// ## Panics
    /// If there is no row for the given ID
    fn row_mut(&'t mut self, id: Self::RowId) -> RowRef<'t, Self::RowMut> {
        self.get_row_mut(id).expect("row not found")
    }

    /// Attempts to get a row by its ID.  
    /// If there is no row for the given ID, this returns [`None`].
    ///
    /// Note: the ID is the row's numerical ID, which could be different
    /// from the index of the row in the table's row list. That is because
    /// BDAT tables can have arbitrary start IDs.
    fn get_row(&'t self, id: Self::RowId) -> Option<RowRef<'t, Self::Row>>;

    /// Attempts to get a mutable view of a row by its ID.  
    /// If there is no row for the given ID, this returns [`None`].
    ///
    /// Note: the ID is the row's numerical ID, which could be different
    /// from the index of the row in the table's row list. That is because
    /// BDAT tables can have arbitrary start IDs.
    fn get_row_mut(&'t mut self, id: Self::RowId) -> Option<RowRef<'t, Self::RowMut>>;

    /// Gets the number of rows in the table
    fn row_count(&self) -> usize;

    /// Gets the number of columns in the table
    fn column_count(&self) -> usize;
}
