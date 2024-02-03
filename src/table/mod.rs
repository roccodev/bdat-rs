//! BDAT table, row, cell implementations

use crate::{
    BdatResult, BdatVersion, Cell, ColumnDef, ColumnMap, Label, Row, RowRef, RowRefMut, ValueType,
};
use thiserror::Error;
use util::VersionedIter;

pub mod cell;
pub mod column;
pub mod row;

mod legacy;
mod modern;
mod util;

pub use legacy::LegacyTable;
pub use modern::ModernTable;

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

/// A builder interface for [`Table`].
pub struct TableBuilder<'b> {
    name: Label,
    columns: ColumnMap,
    rows: Vec<Row<'b>>,
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
    /// The returned cell type for row queries
    type Cell;

    /// Returns the table's name.
    fn name(&self) -> &Label;

    /// Updates the table's name.
    fn set_name(&mut self, name: Label);

    /// Gets the minimum row ID in the table.
    fn base_id(&self) -> usize;

    /// Gets a row by its ID.
    ///
    /// ## Panics
    /// If there is no row for the given ID.
    fn row(&'t self, id: usize) -> RowRef<'t, 'b, Self::Cell> {
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
    fn row_mut(&'t mut self, id: usize) -> RowRefMut<'t, 'b> {
        self.get_row_mut(id).expect("row not found")
    }

    /// Attempts to get a row by its ID.  
    /// If there is no row for the given ID, this returns [`None`].
    ///
    /// Note: the ID is the row's numerical ID, which could be different
    /// from the index of the row in the table's row list. That is because
    /// BDAT tables can have arbitrary start IDs.
    fn get_row(&'t self, id: usize) -> Option<RowRef<'t, 'b, Self::Cell>>;

    /// Attempts to get a mutable view of a row by its ID.  
    /// If there is no row for the given ID, this returns [`None`].
    ///
    /// Note: the ID is the row's numerical ID, which could be different
    /// from the index of the row in the table's row list. That is because
    /// BDAT tables can have arbitrary start IDs.
    fn get_row_mut(&'t mut self, id: usize) -> Option<RowRefMut<'t, 'b>>;

    /// Gets the number of rows in the table
    fn row_count(&self) -> usize;

    /// Gets the number of columns in the table
    fn column_count(&self) -> usize;
}

macro_rules! versioned {
    ($var:expr, $name:ident) => {
        match $var {
            TableInner::Modern(m) => &m.$name,
            TableInner::Legacy(l) => &l.$name,
        }
    };
    ($var:expr, $name:ident($($par:expr ) *)) => {
        match $var {
            TableInner::Modern(m) => m . $name ( $($par, )* ),
            TableInner::Legacy(l) => l . $name ( $($par, )* ),
        }
    };
}

macro_rules! versioned_iter {
    ($var:expr, $name:ident($($par:expr ) *)) => {
        match $var {
            TableInner::Modern(m) => util::VersionedIter::Modern(m . $name ( $($par, )* )),
            TableInner::Legacy(l) => util::VersionedIter::Legacy(l . $name ( $($par, )* )),
        }
    };
}

impl<'b> Table<'b> {
    /// If the table is modern, returns a view of the underlying table.
    ///
    /// ## Panics
    /// Panics if the table is not modern.
    pub fn as_modern(&self) -> &ModernTable<'b> {
        match &self.inner {
            TableInner::Modern(m) => m,
            _ => panic!("not modern"),
        }
    }

    /// If the table is legacy, returns a view of the underlying table.
    ///
    /// ## Panics
    /// Panics if the table is not legacy.
    pub fn as_legacy(&self) -> &LegacyTable<'b> {
        match &self.inner {
            TableInner::Legacy(l) => l,
            _ => panic!("not legacy"),
        }
    }

    /// If the table is modern, returns a mutable view of the underlying table.
    ///
    /// ## Panics
    /// Panics if the table is not modern.
    pub fn as_modern_mut(&mut self) -> &mut ModernTable<'b> {
        match &mut self.inner {
            TableInner::Modern(m) => m,
            _ => panic!("not modern"),
        }
    }

    /// If the table is legacy, returns a mutable view of the underlying table.
    ///
    /// ## Panics
    /// Panics if the table is not legacy.
    pub fn as_legacy_mut(&mut self) -> &mut LegacyTable<'b> {
        match &mut self.inner {
            TableInner::Legacy(l) => l,
            _ => panic!("not legacy"),
        }
    }

    /// If the table is modern, returns the underlying table.
    ///
    /// ## Panics
    /// Panics if the table is not modern.  
    /// For a panic-free function that converts instead, use [`to_modern`].
    ///
    /// [`to_modern`]: Table::to_modern
    pub fn into_modern(self) -> ModernTable<'b> {
        match self.inner {
            TableInner::Modern(m) => m,
            _ => panic!("not modern"),
        }
    }

    /// If the table is legacy, returns the underlying table.
    ///
    /// ## Panics
    /// Panics if the table is not legacy.  
    /// For a panic-free function that converts instead, use [`to_legacy`].
    ///
    /// [`to_legacy`]: Table::to_legacy
    pub fn into_legacy(self) -> LegacyTable<'b> {
        match self.inner {
            TableInner::Legacy(l) => l,
            _ => panic!("not legacy"),
        }
    }

    /// Returns whether the underlying table is modern.
    pub fn is_modern(&self) -> bool {
        matches!(self.inner, TableInner::Modern(_))
    }

    /// Returns whether the underlying table is legacy.
    pub fn is_legacy(&self) -> bool {
        matches!(self.inner, TableInner::Legacy(_))
    }

    /// Returns a modern table as close to the underlying table as possible.
    ///
    /// * If the table is modern, this does nothing and returns it.
    /// * If the table is legacy, it tries to convert it to the
    /// modern format, and returns the result.
    ///
    /// This is not to be confused with [`into_modern`], which panics if
    /// the table is not modern.
    ///
    /// [`into_modern`]: Table::into_modern
    pub fn to_modern(self) -> BdatResult<ModernTable<'b>> {
        match self.inner {
            TableInner::Modern(m) => Ok(m),
            TableInner::Legacy(l) => Ok(l.try_into()?),
        }
    }

    /// Returns a legacy table as close to the underlying table as possible.
    ///
    /// * If the table is legacy, this does nothing and returns it.
    /// * If the table is modern, it tries to convert it to the
    /// legacy format, and returns the result.
    ///
    /// This is not to be confused with [`into_legacy`], which panics if
    /// the table is not legacy.
    ///
    /// [`into_legacy`]: Table::into_legacy
    pub fn to_legacy(self) -> BdatResult<LegacyTable<'b>> {
        match self.inner {
            TableInner::Modern(m) => Ok(m.try_into()?),
            TableInner::Legacy(l) => Ok(l),
        }
    }

    /// Gets an iterator that visits this table's rows
    pub fn rows(&self) -> impl Iterator<Item = RowRef<'_, 'b>> {
        match &self.inner {
            TableInner::Modern(m) => VersionedIter::Modern(m.rows().map(RowRef::up_cast)),
            TableInner::Legacy(l) => VersionedIter::Legacy(l.rows()),
        }
    }

    /// Gets an iterator over mutable references to this table's
    /// rows.
    ///
    /// The iterator does not allow structural modifications to the table. To add, remove, or
    /// reorder rows, convert the table to a new builder first. (`TableBuilder::from(table)`)
    ///
    /// Additionally, if the iterator is used to replace rows, proper care must be taken to
    /// ensure the new rows have the same IDs, as to preserve the original table's row order.
    ///
    /// When the `hash-table` feature is enabled, the new rows must also retain their original
    /// hashed ID (for modern BDATs). Failure to do so will lead to improper behavior of
    /// [`get_row_by_hash`].
    ///
    /// [`get_row_by_hash`]: ModernTable::get_row_by_hash
    pub fn rows_mut(&mut self) -> impl Iterator<Item = RowRefMut<'_, 'b>> {
        versioned_iter!(&mut self.inner, rows_mut())
    }

    /// Gets an owning iterator over this table's rows
    pub fn into_rows(self) -> impl Iterator<Item = Row<'b>> {
        versioned_iter!(self.inner, into_rows())
    }

    /// Gets an iterator that visits this table's column definitions
    pub fn columns(&self) -> impl Iterator<Item = &ColumnDef> {
        versioned_iter!(&self.inner, columns())
    }

    /// Gets an iterator over mutable references to this table's
    /// column definitions.
    pub fn columns_mut(&mut self) -> impl Iterator<Item = &mut ColumnDef> {
        versioned_iter!(&mut self.inner, columns_mut())
    }

    /// Gets an owning iterator over this table's column definitions
    pub fn into_columns(self) -> impl Iterator<Item = ColumnDef> {
        versioned_iter!(self.inner, into_columns())
    }
}

impl<'t, 'b: 't> TableAccessor<'t, 'b> for Table<'b> {
    type Cell = &'t Cell<'b>;

    fn name(&self) -> &Label {
        versioned!(&self.inner, name)
    }

    fn set_name(&mut self, name: Label) {
        versioned!(&mut self.inner, set_name(name))
    }

    fn base_id(&self) -> usize {
        *versioned!(&self.inner, base_id)
    }

    fn row(&self, id: usize) -> RowRef<'_, 'b> {
        match &self.inner {
            TableInner::Modern(m) => m.row(id).up_cast(),
            TableInner::Legacy(l) => l.row(id),
        }
    }

    fn row_mut(&mut self, id: usize) -> RowRefMut<'_, 'b> {
        versioned!(&mut self.inner, row_mut(id))
    }

    fn get_row(&self, id: usize) -> Option<RowRef<'_, 'b>> {
        match &self.inner {
            TableInner::Modern(m) => m.get_row(id).map(RowRef::up_cast),
            TableInner::Legacy(l) => l.get_row(id),
        }
    }

    fn get_row_mut(&mut self, id: usize) -> Option<RowRefMut<'_, 'b>> {
        versioned!(&mut self.inner, get_row_mut(id))
    }

    fn row_count(&self) -> usize {
        versioned!(&self.inner, row_count())
    }

    fn column_count(&self) -> usize {
        versioned!(&self.inner, column_count())
    }
}

impl<'b> TableBuilder<'b> {
    pub fn with_name(name: Label) -> Self {
        Self {
            name,
            columns: ColumnMap::default(),
            rows: vec![],
        }
    }

    pub fn add_column(mut self, column: ColumnDef) -> Self {
        self.columns.push(column);
        self
    }

    /// Adds a new row at the end of the table.
    ///
    /// ## Panics
    /// Panics if the new row ID's isn't exactly the ID of the current
    /// last row + 1, or if no more rows can be added.
    pub fn add_row(mut self, row: Row<'b>) -> Self {
        // ID sanity check
        if let Some(last_row) = self.rows.last() {
            if last_row.id() == u32::MAX as usize {
                panic!("row limit of {} reached, no more rows can be added", u32::MAX);
            }
            if last_row.id() + 1 != row.id() {
                panic!("attempted to add non-consecutive row ID, expected {}, found {}",
                    last_row.id() + 1, row.id());
            }
        }
        self.rows.push(row);
        self
    }

    /// Sets the entire row list for the table.
    ///
    /// ## Panics
    /// Panics if any two consecutive rows have non-consecutive or wrongly
    /// ordered IDs.
    pub fn set_rows(mut self, rows: Vec<Row<'b>>) -> Self {
        for w in rows.windows(2) {
            let [a, b] = w else { continue }; // Only 1 row
            let (a, b) = (a.id(), b.id());
            if a >= b {
                panic!("found pair of wrongly-ordered IDs, {} >= {}", a, b);
            }
            if b - a != 1 {
                panic!("found pair of non-consecutive row IDs, expected {}/{}, found {}/{}",
                    a, a + 1, a, b);
            }
        }
        self.rows = rows;
        self
    }

    pub fn set_columns(mut self, columns: Vec<ColumnDef>) -> Self {
        self.columns = ColumnMap::from(columns);
        self
    }

    pub fn build_modern(self) -> ModernTable<'b> {
        ModernTable::new(self)
    }

    pub fn build_legacy(self) -> LegacyTable<'b> {
        LegacyTable::new(self)
    }

    pub fn build(self, version: BdatVersion) -> Table<'b> {
        if version.is_legacy() {
            self.build_legacy().into()
        } else {
            self.build_modern().into()
        }
    }
}
