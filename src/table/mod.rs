use crate::hash::PreHashedMap;
use crate::{ColumnDef, Label, Row, RowRef};

pub mod cell;
pub mod column;
pub mod row;

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
/// use bdat::{Table, TableBuilder, Cell, ColumnDef, Row, Value, ValueType, Label};
///
/// let table: Table = TableBuilder::with_name(Label::Hash(0xDEADBEEF))
///     .add_column(ColumnDef::new(ValueType::UnsignedInt, Label::Hash(0xCAFEBABE)))
///     .add_row(Row::new(1, vec![Cell::Single(Value::UnsignedInt(10))]))
///     .build();
///
/// assert_eq!(table.row_count(), 1);
/// assert_eq!(
///     *table.row(1)[Label::Hash(0xCAFEBABE)].as_single().unwrap(),
///     Value::UnsignedInt(10)
/// );
/// ```
#[derive(Debug, Clone, PartialEq)]
pub struct Table<'b> {
    pub(crate) name: Label,
    pub(crate) base_id: usize,
    pub(crate) columns: Vec<ColumnDef>,
    pub(crate) rows: Vec<Row<'b>>,
    #[cfg(feature = "hash-table")]
    row_hash_table: PreHashedMap<u32, usize>,
}

/// A builder interface for [`Table`].
pub struct TableBuilder<'b> {
    name: Label,
    columns: Vec<ColumnDef>,
    rows: Vec<Row<'b>>,
}

impl<'b> Table<'b> {
    fn new(builder: TableBuilder<'b>) -> Self {
        Self {
            name: builder.name,
            columns: builder.columns,
            base_id: builder.rows.iter().map(|r| r.id).min().unwrap_or_default(),
            #[cfg(feature = "hash-table")]
            row_hash_table: builder
                .rows
                .iter()
                .filter_map(|r| Some((r.id_hash()?, r.id)))
                .collect(),
            rows: builder.rows,
        }
    }

    /// Returns the table's name.
    pub fn name(&self) -> &Label {
        &self.name
    }

    /// Updates the table's name.
    pub fn set_name(&mut self, name: Label) {
        self.name = name;
    }

    /// Gets the minimum row ID in the table.
    pub fn base_id(&self) -> usize {
        self.base_id
    }

    /// Gets a row by its ID
    ///
    /// Note: the ID is the row's numerical ID, which could be different
    /// from the index of the row in the table's row list. That is because
    /// BDAT tables can have arbitrary start IDs.
    ///
    /// # Panics
    /// If there is no row for the given ID
    pub fn row(&self, id: usize) -> RowRef<'_, 'b> {
        self.get_row(id).expect("no such row")
    }

    /// Attempts to get a row by its ID.  
    /// If there is no row for the given ID, this returns [`None`].
    ///
    /// Note: the ID is the row's numerical ID, which could be different
    /// from the index of the row in the table's row list. That is because
    /// BDAT tables can have arbitrary start IDs.
    pub fn get_row(&self, id: usize) -> Option<RowRef<'_, 'b>> {
        let index = id.checked_sub(self.base_id)?;
        self.rows.get(index).map(|_| RowRef::new(self, index, id))
    }

    /// Gets an iterator that visits this table's rows
    pub fn rows(&self) -> impl Iterator<Item = &Row<'b>> {
        self.rows.iter()
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
    pub fn rows_mut(&mut self) -> impl Iterator<Item = &mut Row<'b>> {
        self.rows.iter_mut()
    }

    /// Gets an owning iterator over this table's rows
    pub fn into_rows(self) -> impl Iterator<Item = Row<'b>> {
        self.rows.into_iter()
    }

    /// Gets an iterator that visits this table's column definitions
    pub fn columns(&self) -> impl Iterator<Item = &ColumnDef> {
        self.columns.iter()
    }

    /// Gets an iterator over mutable references to this table's
    /// column definitions.
    pub fn columns_mut(&mut self) -> impl Iterator<Item = &mut ColumnDef> {
        self.columns.iter_mut()
    }

    /// Gets an owning iterator over this table's column definitions
    pub fn into_columns(self) -> impl Iterator<Item = ColumnDef> {
        self.columns.into_iter()
    }

    /// Gets the number of rows in the table
    pub fn row_count(&self) -> usize {
        self.rows.len()
    }

    /// Gets the number of columns in the table
    pub fn column_count(&self) -> usize {
        self.columns.len()
    }

    /// Attempts to get a row by its hashed 32-bit ID.
    /// If there is no row for the given ID, this returns [`None`].
    ///
    /// This requires the `hash-table` feature flag, which is enabled
    /// by default.
    #[cfg(feature = "hash-table")]
    pub fn get_row_by_hash(&self, hash_id: u32) -> Option<RowRef<'_, 'b>> {
        self.row_hash_table
            .get(&hash_id)
            .and_then(|&id| self.get_row(id))
    }

    /// Returns an ergonomic iterator view over the table's rows and columns.
    pub fn iter(&self) -> RowIter {
        self.into_iter()
    }
}

impl<'b> TableBuilder<'b> {
    pub fn with_name(name: Label) -> Self {
        Self {
            name,
            columns: vec![],
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
        self.columns = columns;
        self
    }

    pub fn build(self) -> Table<'b> {
        Table::new(self)
    }
}

impl<'t, 'tb> IntoIterator for &'t Table<'tb> {
    type Item = RowRef<'t, 'tb>;
    type IntoIter = RowIter<'t, 'tb>;

    fn into_iter(self) -> Self::IntoIter {
        RowIter {
            table: self,
            row_id: self.base_id(),
        }
    }
}

impl<'b> From<Table<'b>> for TableBuilder<'b> {
    fn from(table: Table<'b>) -> Self {
        Self {
            name: table.name,
            columns: table.columns,
            rows: table.rows,
        }
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
            .build();
        assert_eq!(1, table.get_row_by_hash(0xabcdef01).unwrap().id());
        assert_eq!(2, table.get_row_by_hash(0xdeadbeef).unwrap().id());
        assert_eq!(
            256,
            table.get_row_by_hash(0xabcdef01).unwrap()[Label::Hash(1)]
                .as_single()
                .unwrap()
                .clone()
                .to_integer()
        );
        assert_eq!(
            100,
            table.get_row_by_hash(0xdeadbeef).unwrap()[Label::Hash(1)]
                .as_single()
                .unwrap()
                .clone()
                .to_integer()
        );
    }
}

pub struct RowIter<'t, 'tb> {
    table: &'t Table<'tb>,
    row_id: usize,
}

impl<'t, 'tb> Iterator for RowIter<'t, 'tb> {
    type Item = RowRef<'t, 'tb>;

    fn next(&mut self) -> Option<Self::Item> {
        let item = self.table.get_row(self.row_id)?;
        self.row_id += 1;
        Some(item)
    }
}
