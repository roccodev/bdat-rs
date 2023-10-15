use crate::hash::PreHashedMap;
use crate::{
    ColumnDef, ColumnMap, Label, ModernCell, Row, RowRef, RowRefMut, TableAccessor, TableBuilder,
};

#[derive(Debug, Clone, PartialEq)]
pub struct ModernTable<'b> {
    pub(crate) name: Label,
    pub(crate) base_id: usize,
    pub(crate) columns: ColumnMap,
    pub(crate) rows: Vec<Row<'b>>,
    #[cfg(feature = "hash-table")]
    row_hash_table: PreHashedMap<u32, usize>,
}

impl<'b> ModernTable<'b> {
    pub(crate) fn new(builder: TableBuilder<'b>) -> Self {
        Self {
            name: builder.name,
            columns: builder.columns,
            base_id: builder
                .rows
                .iter()
                .map(|r| r.id())
                .min()
                .unwrap_or_default(),
            #[cfg(feature = "hash-table")]
            row_hash_table: builder
                .rows
                .iter()
                .filter_map(|r| Some((r.id_hash()?, r.id())))
                .collect(),
            rows: builder.rows,
        }
    }

    /// Attempts to get a row by its hashed 32-bit ID.
    /// If there is no row for the given ID, this returns [`None`].
    ///
    /// This requires the `hash-table` feature flag, which is enabled
    /// by default.
    #[cfg(feature = "hash-table")]
    pub fn get_row_by_hash(&self, hash_id: u32) -> Option<RowRef<'_, 'b, ModernCell<'_, 'b>>> {
        self.row_hash_table
            .get(&hash_id)
            .and_then(|&id| self.get_row(id))
    }

    /// Gets an iterator that visits this table's rows
    pub fn rows(&self) -> impl Iterator<Item = RowRef<'_, 'b, ModernCell<'_, 'b>>> {
        self.rows.iter().map(|row| RowRef::new(row, &self.columns))
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
        self.rows
            .iter_mut()
            .map(|row| RowRefMut::new(row, &self.columns))
    }

    /// Gets an owning iterator over this table's rows
    pub fn into_rows(self) -> impl Iterator<Item = Row<'b>> {
        self.rows.into_iter()
    }

    /// Gets an iterator that visits this table's column definitions
    pub fn columns(&self) -> impl Iterator<Item = &ColumnDef> {
        self.columns.as_slice().iter()
    }

    /// Gets an iterator over mutable references to this table's
    /// column definitions.
    pub fn columns_mut(&mut self) -> impl Iterator<Item = &mut ColumnDef> {
        self.columns.as_mut_slice().iter_mut()
    }

    /// Gets an owning iterator over this table's column definitions
    pub fn into_columns(self) -> impl Iterator<Item = ColumnDef> {
        self.columns.into_raw().into_iter()
    }
}

impl<'t, 'b: 't> TableAccessor<'t, 'b> for ModernTable<'b> {
    type Cell = ModernCell<'t, 'b>;

    fn name(&self) -> &Label {
        &self.name
    }

    fn set_name(&mut self, name: Label) {
        self.name = name;
    }

    fn base_id(&self) -> usize {
        self.base_id
    }

    /// Gets a row by its ID.
    ///
    /// ## Panics
    /// If there is no row for the given ID.
    ///
    /// ## Example
    /// ```
    /// use bdat::{Label, ModernTable, TableAccessor};
    ///
    /// fn foo(table: &ModernTable) -> u32 {
    ///     // This is a `ModernCell`, which is essentially a single value.
    ///     // As such, it can be used to avoid having to match on single-value cells
    ///     // that are included for legacy compatibility.
    ///     let cell = table.row(1).get(Label::Hash(0xDEADBEEF));
    ///     // Casting values is also supported:
    ///     cell.get_as::<u32>()
    /// }
    /// ```
    fn row(&self, id: usize) -> RowRef<'_, 'b, ModernCell<'_, 'b>> {
        self.get_row(id).expect("no such row")
    }

    fn row_mut(&mut self, id: usize) -> RowRefMut<'_, 'b> {
        self.get_row_mut(id).expect("no such row")
    }

    fn get_row(&self, id: usize) -> Option<RowRef<'_, 'b, ModernCell<'_, 'b>>> {
        let index = id.checked_sub(self.base_id)?;
        self.rows
            .get(index)
            .map(|row| RowRef::new(row, &self.columns))
    }

    fn get_row_mut(&mut self, id: usize) -> Option<RowRefMut<'_, 'b>> {
        let index = id.checked_sub(self.base_id)?;
        self.rows
            .get_mut(index)
            .map(|row| RowRefMut::new(row, &self.columns))
    }

    fn row_count(&self) -> usize {
        self.rows.len()
    }

    fn column_count(&self) -> usize {
        self.columns.as_slice().len()
    }
}
