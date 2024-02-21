use crate::hash::PreHashedMap;
use crate::{
    BdatVersion, Cell, CellAccessor, ColumnDef, ColumnMap, Label, LegacyTable, LegacyTableBuilder,
    ModernTableBuilder, RowId, RowRef, Table, TableBuilder, Value,
};

use super::util::EnumId;
use super::{FormatConvertError, TableInner};

/// The BDAT table representation in modern formats, currently used in Xenoblade 3.
///
/// # Characteristics
///
/// ## Hashed labels
///
/// Modern tables use hashed labels for table and column names.
///
/// Additionally, rows might have an ID field that can be used to quickly find them.
/// This ID is exposed via [`get_row_by_hash`] and [`row_by_hash`].
///
/// ## Simpler cells
///
/// Unlike legacy tables, modern tables only support single-value cells (i.e. [`Cell::Single`]).
/// Rows queried from this struct return [`ModernCell`], letting you directly operate on values.
///
/// # Examples
///
/// ## Getting a row by its hashed ID
///
/// Note: this requires the `hash-table` feature flag, which is enabled by default.
///
/// ```
/// use bdat::ModernTable;
///
/// fn foo(table: &ModernTable) {
///     let row = table.row_by_hash(0xDEADBEEF);
///     assert_eq!(0xDEADBEEF, row.id_hash().unwrap());
/// }
/// ```
///
/// ## Operating on single-value cells
///
/// ```
/// use bdat::{Label, ModernTable, label_hash};
///
/// fn get_character_id(table: &ModernTable, row_id: u32) -> u32 {
///     table.row(row_id).get(label_hash!("CharacterID")).get_as()
/// }
/// ```
///
/// [`get_row_by_hash`]: ModernTable::get_row_by_hash
/// [`row_by_hash`]: ModernTable::row_by_hash
/// [`ModernCell`]: crate::ModernCell
/// [`Cell::Single`]: crate::Cell::Single
#[derive(Debug, Clone, PartialEq)]
pub struct ModernTable<'b> {
    pub(crate) name: Label,
    pub(crate) base_id: u32,
    pub(crate) columns: ColumnMap,
    pub(crate) rows: Vec<ModernRow<'b>>,
    #[cfg(feature = "hash-table")]
    row_hash_table: PreHashedMap<u32, RowId>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ModernRow<'b> {
    pub(crate) values: Vec<Value<'b>>,
}

impl<'b> ModernTable<'b> {
    pub(crate) fn new(builder: ModernTableBuilder<'b>) -> Self {
        Self {
            name: builder.name,
            columns: builder.columns,
            base_id: builder.base_id,
            #[cfg(feature = "hash-table")]
            row_hash_table: build_id_map_checked(&builder.rows, builder.base_id),
            rows: builder.rows,
        }
    }

    pub fn name(&self) -> &Label {
        &self.name
    }

    pub fn set_name(&mut self, name: Label) {
        self.name = name;
    }

    /// Gets the minimum row ID in the table.
    pub fn base_id(&self) -> RowId {
        self.base_id
    }

    /// Gets a row by its ID.
    ///
    /// Note: the ID is the row's numerical ID, which could be different
    /// from the index of the row in the table's row list. That is because
    /// BDAT tables can have arbitrary start IDs.
    ///
    /// ## Panics
    /// If there is no row for the given ID.
    ///
    /// ## Example
    /// ```
    /// use bdat::{Label, ModernTable};
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
    pub fn row(&self, id: RowId) -> RowRef<&ModernRow<'b>> {
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
    pub fn row_mut(&mut self, id: RowId) -> RowRef<&mut ModernRow<'b>> {
        self.get_row_mut(id).expect("row not found")
    }

    /// Attempts to get a row by its ID.  
    /// If there is no row for the given ID, this returns [`None`].
    ///
    /// Note: the ID is the row's numerical ID, which could be different
    /// from the index of the row in the table's row list. That is because
    /// BDAT tables can have arbitrary start IDs.
    pub fn get_row(&self, id: RowId) -> Option<RowRef<&ModernRow<'b>>> {
        let index = id.checked_sub(self.base_id)?;
        self.rows
            .get(index as usize)
            .map(move |row| RowRef::new(id, row, &self.columns))
    }

    /// Attempts to get a mutable view of a row by its ID.  
    /// If there is no row for the given ID, this returns [`None`].
    ///
    /// Note: the ID is the row's numerical ID, which could be different
    /// from the index of the row in the table's row list. That is because
    /// BDAT tables can have arbitrary start IDs.
    pub fn get_row_mut(&mut self, id: RowId) -> Option<RowRef<&mut ModernRow<'b>>> {
        let index = id.checked_sub(self.base_id)?;
        self.rows
            .get_mut(index as usize)
            .map(|row| RowRef::new(id, row, &self.columns))
    }

    /// Attempts to get a row by its hashed 32-bit ID.
    /// If there is no row for the given ID, this returns [`None`].
    ///
    /// This requires the `hash-table` feature flag, which is enabled
    /// by default.
    #[cfg(feature = "hash-table")]
    pub fn get_row_by_hash(&self, hash_id: u32) -> Option<RowRef<'_, &ModernRow<'b>>> {
        self.row_hash_table
            .get(&hash_id)
            .and_then(|&id| self.get_row(id))
    }

    /// Gets a row by its hashed 32-bit ID.
    ///
    /// This requires the `hash-table` feature flag, which is enabled
    /// by default.
    ///
    /// ## Panics
    /// Panics if there is no row for the given ID.
    #[cfg(feature = "hash-table")]
    pub fn row_by_hash(&self, hash_id: u32) -> RowRef<'_, &ModernRow<'b>> {
        self.get_row_by_hash(hash_id)
            .expect("no row with given hash")
    }

    /// Gets an iterator that visits this table's rows
    pub fn rows(&self) -> impl Iterator<Item = RowRef<'_, &ModernRow<'b>>> {
        self.rows
            .iter()
            .enum_id(self.base_id)
            .map(|(id, row)| RowRef::new(id, row, &self.columns))
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
    /// hashed ID. Failure to do so will lead to improper behavior of
    /// [`get_row_by_hash`].
    ///
    /// [`get_row_by_hash`]: ModernTable::get_row_by_hash
    pub fn rows_mut(&mut self) -> impl Iterator<Item = RowRef<'_, &mut ModernRow<'b>>> {
        self.rows
            .iter_mut()
            .enum_id(self.base_id)
            .map(|(id, row)| RowRef::new(id, row, &self.columns))
    }

    /// Gets an owning iterator over this table's rows
    pub fn into_rows(self) -> impl Iterator<Item = ModernRow<'b>> {
        self.rows.into_iter()
    }

    /// Gets an owning iterator over this table's rows, in pairs of
    /// `(row ID, row)`.
    pub fn into_rows_id(self) -> impl Iterator<Item = (u32, ModernRow<'b>)> {
        self.rows.into_iter().enum_id(self.base_id)
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

    pub fn row_count(&self) -> usize {
        self.rows.len()
    }

    pub fn column_count(&self) -> usize {
        self.columns.as_slice().len()
    }
}

impl<'b> ModernRow<'b> {
    pub fn new(values: Vec<Value<'b>>) -> Self {
        Self { values }
    }

    /// Gets an owning iterator over this row's values
    pub fn into_values(self) -> impl Iterator<Item = Value<'b>> {
        self.values.into_iter()
    }

    /// Gets an iterator over this row's values
    pub fn values(&self) -> impl Iterator<Item = &Value<'b>> {
        self.values.iter()
    }

    /// Searches the row's cells for a ID hash field, returning the ID
    /// of this row if found.
    pub fn id_hash(&self) -> Option<RowId> {
        self.values.iter().find_map(|value| match value {
            Value::HashRef(id) => Some(*id),
            _ => None,
        })
    }
}

/// Builds a primary key index for the table.
///
/// If there is no hash-type column, the map will be empty.
///
/// ## Panics
/// Panics if there are two rows with the same key hash.
#[cfg(feature = "hash-table")]
fn build_id_map_checked(rows: &[ModernRow], base_id: u32) -> PreHashedMap<u32, RowId> {
    use std::collections::hash_map::Entry;

    let mut res = PreHashedMap::with_capacity_and_hasher(rows.len(), Default::default());
    for (id, row) in rows.iter().enum_id(base_id) {
        let Some(hash) = row.id_hash() else { continue };
        match res.entry(hash) {
            Entry::Occupied(_) => panic!(
                "failed to build row hash table: duplicate key {:?}",
                Label::Hash(hash)
            ),
            e => e.or_insert(id),
        };
    }
    res
}

impl<'a, 'b> CellAccessor for &'a ModernRow<'b> {
    type Target = &'a Value<'b>;

    fn access(self, pos: usize) -> Option<Self::Target> {
        self.values.get(pos)
    }
}

impl<'a, 'b> CellAccessor for &'a mut ModernRow<'b> {
    type Target = &'a mut Value<'b>;

    fn access(self, pos: usize) -> Option<Self::Target> {
        self.values.get_mut(pos)
    }
}

impl<'b> From<ModernTable<'b>> for ModernTableBuilder<'b> {
    fn from(value: ModernTable<'b>) -> Self {
        Self::from_table(value.name, value.base_id, value.columns, value.rows)
    }
}

impl<'b> From<ModernTable<'b>> for TableBuilder<'b> {
    fn from(value: ModernTable<'b>) -> Self {
        Self::from(ModernTableBuilder::from(value))
    }
}

impl<'b> From<ModernTable<'b>> for Table<'b> {
    fn from(value: ModernTable<'b>) -> Self {
        Self {
            inner: TableInner::Modern(value),
        }
    }
}

/// Legacy -> Modern conversion
impl<'b> TryFrom<LegacyTable<'b>> for ModernTable<'b> {
    type Error = FormatConvertError;

    fn try_from(value: LegacyTable<'b>) -> Result<Self, Self::Error> {
        TableBuilder::from(value).to_modern()?.try_build()
    }
}

#[cfg(test)]
mod tests {
    #[cfg(feature = "hash-table")]
    #[test]
    fn test_hash_table() {
        use crate::{ColumnDef, Label, ModernRow, ModernTableBuilder, Value, ValueType};

        let table = ModernTableBuilder::with_name(Label::Hash(0xDEADBEEF))
            .set_base_id(1)
            .add_column(ColumnDef::new(ValueType::HashRef, 0.into()))
            .add_column(ColumnDef::new(ValueType::UnsignedInt, 1.into()))
            .add_row(ModernRow::new(vec![
                Value::HashRef(0xabcdef01),
                Value::UnsignedInt(256),
            ]))
            .add_row(ModernRow::new(vec![
                Value::HashRef(0xdeadbeef),
                Value::UnsignedInt(100),
            ]))
            .build();
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
