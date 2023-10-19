use crate::{
    BdatVersion, ColumnDef, ColumnMap, Label, LegacyCell, ModernTable, Row, RowRef, RowRefMut,
    Table, TableAccessor, TableBuilder,
};

use super::{FormatConvertError, TableInner};

/// The BDAT table representation in legacy formats, used for all games before Xenoblade 3.
///
/// # Characteristics
///
/// ## Cell types
///
/// Unlike modern tables, legacy tables can have multiple-value cells, or mask a value's bits
/// to create flags.
///
/// See also: [`LegacyCell`]
///
/// # Examples
///
/// ## Operating on cells
///
/// ```
/// use bdat::{Label, LegacyTable, TableAccessor, label_hash};
///
/// fn get_character_id(table: &LegacyTable, row_id: usize) -> u32 {
///     let cell = table.row(row_id).get(Label::from("CharacterID"));
///     // Unlike modern tables, we can't simply operate on the value.
///     // We can `match` on cell types, or simply cast them and handle errors:
///     cell.as_single().unwrap().get_as::<u32>()
/// }
/// ```
#[derive(Debug, Clone, PartialEq)]
pub struct LegacyTable<'b> {
    pub(crate) name: Label,
    pub(crate) base_id: usize,
    pub(crate) columns: ColumnMap,
    pub(crate) rows: Vec<Row<'b>>,
}

impl<'b> LegacyTable<'b> {
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
            rows: builder.rows,
        }
    }

    /// Gets an iterator that visits this table's rows
    pub fn rows(&self) -> impl Iterator<Item = RowRef<'_, 'b, LegacyCell<'_, 'b>>> {
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

impl<'t, 'b: 't> TableAccessor<'t, 'b> for LegacyTable<'b> {
    type Cell = LegacyCell<'t, 'b>;

    fn name(&self) -> &Label {
        &self.name
    }

    fn set_name(&mut self, name: Label) {
        self.name = name;
    }

    fn base_id(&self) -> usize {
        self.base_id
    }

    fn get_row(&self, id: usize) -> Option<RowRef<'_, 'b, LegacyCell<'_, 'b>>> {
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

impl<'b> From<LegacyTable<'b>> for TableBuilder<'b> {
    fn from(value: LegacyTable<'b>) -> Self {
        Self {
            name: value.name,
            columns: value.columns,
            rows: value.rows,
        }
    }
}

impl<'b> From<LegacyTable<'b>> for Table<'b> {
    fn from(value: LegacyTable<'b>) -> Self {
        Self {
            inner: TableInner::Legacy(value),
        }
    }
}

/// Modern -> Legacy conversion
impl<'b> TryFrom<ModernTable<'b>> for LegacyTable<'b> {
    type Error = FormatConvertError;

    fn try_from(value: ModernTable<'b>) -> Result<Self, Self::Error> {
        // any legacy version works here
        if let Some(col) = value
            .columns()
            .find(|c| !c.value_type().is_supported(BdatVersion::LegacySwitch))
        {
            return Err(FormatConvertError::UnsupportedValueType(col.value_type()));
        }
        Ok(LegacyTable::new(TableBuilder::from(value)))
    }
}
