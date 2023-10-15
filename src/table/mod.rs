use crate::{
    BdatVersion, Cell, ColumnDef, ColumnMap, Label, LegacyCell, ModernCell, Row, RowRef, RowRefMut,
};

pub mod cell;
pub mod column;
pub mod row;

mod legacy;
mod modern;
mod util;

use crate::table::util::VersionedIter;
pub use legacy::LegacyTable;
pub use modern::ModernTable;

/// A Bdat table. Depending on how they were read, BDAT tables can either own their data source
/// or borrow from it.
///
/// ## Accessing cells
/// The [`Table::row`] function provides an easy interface to access cells.
/// For example, to access the cell at row 1 and column "Param1", you can use `table.row(1)["Param1"]`.
///
/// See also: [`RowRef`]
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
///     *table.row(1)[Label::Hash(0xCAFEBABE)].as_single().unwrap(),
///     Value::UnsignedInt(10)
/// );
/// ```
#[derive(Debug, Clone, PartialEq)]
pub enum Table<'b> {
    Modern(ModernTable<'b>),
    Legacy(LegacyTable<'b>),
}

/// A builder interface for [`Table`].
pub struct TableBuilder<'b> {
    name: Label,
    columns: ColumnMap,
    rows: Vec<Row<'b>>,
}

pub struct RowIter<'t, T> {
    table: &'t T,
    row_id: usize,
}

pub trait TableAccessor<'t, 'b: 't> {
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
    fn row(&'t self, id: usize) -> RowRef<'t, 'b, Self::Cell>;

    /// Gets a mutable view of a row by its ID
    ///
    /// Note: the ID is the row's numerical ID, which could be different
    /// from the index of the row in the table's row list. That is because
    /// BDAT tables can have arbitrary start IDs.
    ///
    /// ## Panics
    /// If there is no row for the given ID
    fn row_mut(&'t mut self, id: usize) -> RowRefMut<'t, 'b>;

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
            Self::Modern(m) => &m.$name,
            Self::Legacy(l) => &l.$name,
        }
    };
    ($var:expr, $name:ident($($par:expr ) *)) => {
        match $var {
            Self::Modern(m) => m . $name ( $($par, )* ),
            Self::Legacy(l) => l . $name ( $($par, )* ),
        }
    };
}

macro_rules! versioned_iter {
    ($var:expr, $name:ident($($par:expr ) *)) => {
        match $var {
            Self::Modern(m) => util::VersionedIter::Modern(m . $name ( $($par, )* )),
            Self::Legacy(l) => util::VersionedIter::Legacy(l . $name ( $($par, )* )),
        }
    };
}

impl<'b> Table<'b> {
    pub fn as_modern(&self) -> &ModernTable {
        match self {
            Table::Modern(m) => m,
            _ => panic!("not modern"),
        }
    }

    pub fn as_legacy(&self) -> &LegacyTable {
        match self {
            Table::Legacy(l) => l,
            _ => panic!("not legacy"),
        }
    }

    pub fn into_modern(self) -> ModernTable<'b> {
        match self {
            Table::Modern(m) => m,
            _ => panic!("not modern"),
        }
    }

    pub fn into_legacy(self) -> LegacyTable<'b> {
        match self {
            Table::Legacy(l) => l,
            _ => panic!("not legacy"),
        }
    }

    /// Gets an iterator that visits this table's rows
    pub fn rows(&self) -> impl Iterator<Item = RowRef<'_, 'b>> {
        match self {
            Table::Modern(m) => VersionedIter::Modern(m.rows().map(RowRef::up_cast)),
            Table::Legacy(l) => VersionedIter::Legacy(l.rows().map(RowRef::up_cast)),
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
    /// [`get_row_by_hash`]: Table::get_row_by_hash
    pub fn rows_mut(&mut self) -> impl Iterator<Item = RowRefMut<'_, 'b>> {
        versioned_iter!(self, rows_mut())
    }

    /// Gets an owning iterator over this table's rows
    pub fn into_rows(self) -> impl Iterator<Item = Row<'b>> {
        versioned_iter!(self, into_rows())
    }

    /// Gets an iterator that visits this table's column definitions
    pub fn columns(&self) -> impl Iterator<Item = &ColumnDef> {
        versioned_iter!(self, columns())
    }

    /// Gets an iterator over mutable references to this table's
    /// column definitions.
    pub fn columns_mut(&mut self) -> impl Iterator<Item = &mut ColumnDef> {
        versioned_iter!(self, columns_mut())
    }

    /// Gets an owning iterator over this table's column definitions
    pub fn into_columns(self) -> impl Iterator<Item = ColumnDef> {
        versioned_iter!(self, into_columns())
    }
}

impl<'t, 'b: 't> TableAccessor<'t, 'b> for Table<'b> {
    type Cell = &'t Cell<'b>;

    fn name(&self) -> &Label {
        versioned!(self, name)
    }

    fn set_name(&mut self, name: Label) {
        versioned!(self, set_name(name))
    }

    fn base_id(&self) -> usize {
        *versioned!(self, base_id)
    }

    fn row(&self, id: usize) -> RowRef<'_, 'b> {
        match self {
            Table::Modern(m) => m.row(id).up_cast(),
            Table::Legacy(l) => l.row(id).up_cast(),
        }
    }

    fn row_mut(&mut self, id: usize) -> RowRefMut<'_, 'b> {
        versioned!(self, row_mut(id))
    }

    fn get_row(&self, id: usize) -> Option<RowRef<'_, 'b>> {
        match self {
            Table::Modern(m) => m.get_row(id).map(RowRef::up_cast),
            Table::Legacy(l) => l.get_row(id).map(RowRef::up_cast),
        }
    }

    fn get_row_mut(&mut self, id: usize) -> Option<RowRefMut<'_, 'b>> {
        versioned!(self, get_row_mut(id))
    }

    fn row_count(&self) -> usize {
        versioned!(self, row_count())
    }

    fn column_count(&self) -> usize {
        versioned!(self, column_count())
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

    pub fn add_row(mut self, row: Row<'b>) -> Self {
        self.rows.push(row);
        self
    }

    pub fn set_rows(mut self, rows: Vec<Row<'b>>) -> Self {
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

impl<'t, 'tb> IntoIterator for &'t ModernTable<'tb> {
    type Item = RowRef<'t, 'tb, ModernCell<'t, 'tb>>;
    type IntoIter = RowIter<'t, ModernTable<'tb>>;

    fn into_iter(self) -> Self::IntoIter {
        RowIter {
            table: self,
            row_id: self.base_id(),
        }
    }
}

impl<'t, 'tb> IntoIterator for &'t LegacyTable<'tb> {
    type Item = RowRef<'t, 'tb, LegacyCell<'t, 'tb>>;
    type IntoIter = RowIter<'t, LegacyTable<'tb>>;

    fn into_iter(self) -> Self::IntoIter {
        RowIter {
            table: self,
            row_id: self.base_id(),
        }
    }
}

impl<'b> From<ModernTable<'b>> for TableBuilder<'b> {
    fn from(table: ModernTable<'b>) -> Self {
        Self {
            name: table.name,
            columns: table.columns,
            rows: table.rows,
        }
    }
}

impl<'b> From<ModernTable<'b>> for Table<'b> {
    fn from(value: ModernTable<'b>) -> Self {
        Self::Modern(value)
    }
}

impl<'b> From<LegacyTable<'b>> for Table<'b> {
    fn from(value: LegacyTable<'b>) -> Self {
        Self::Legacy(value)
    }
}

impl<'t, 'tb> Iterator for RowIter<'t, ModernTable<'tb>> {
    type Item = RowRef<'t, 'tb, ModernCell<'t, 'tb>>;

    fn next(&mut self) -> Option<Self::Item> {
        let item = self.table.get_row(self.row_id)?;
        self.row_id += 1;
        Some(item)
    }
}

// TODO: trait for get_row
impl<'t, 'tb> Iterator for RowIter<'t, LegacyTable<'tb>> {
    type Item = RowRef<'t, 'tb, LegacyCell<'t, 'tb>>;

    fn next(&mut self) -> Option<Self::Item> {
        let item = self.table.get_row(self.row_id)?;
        self.row_id += 1;
        Some(item)
    }
}

#[cfg(test)]
mod tests {
    #[cfg(feature = "hash-table")]
    #[test]
    fn test_hash_table() {
        use crate::{Cell, ColumnDef, Label, Row, TableBuilder, Value, ValueType};

        let table = TableBuilder::with_name(Label::Hash(0xDEADBEEF))
            .add_column(ColumnDef::new(ValueType::HashRef, 0.into()))
            .add_column(ColumnDef::new(ValueType::UnsignedInt, 1.into()))
            .add_row(Row::new(
                1,
                vec![
                    Cell::Single(Value::HashRef(0xabcdef01)),
                    Cell::Single(Value::UnsignedInt(256)),
                ],
            ))
            .add_row(Row::new(
                2,
                vec![
                    Cell::Single(Value::HashRef(0xdeadbeef)),
                    Cell::Single(Value::UnsignedInt(100)),
                ],
            ))
            .build_modern();
        assert_eq!(1, table.get_row_by_hash(0xabcdef01).unwrap().id());
        assert_eq!(2, table.get_row_by_hash(0xdeadbeef).unwrap().id());
        assert_eq!(
            256,
            table
                .get_row_by_hash(0xabcdef01)
                .unwrap()
                .get(Label::Hash(1))
                .get_as::<u32>()
        );
        assert_eq!(
            100,
            table
                .get_row_by_hash(0xdeadbeef)
                .unwrap()
                .get(Label::Hash(1))
                .get_as::<u32>()
        );
    }
}
