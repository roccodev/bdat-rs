use crate::{
    BdatVersion, Cell, CellAccessor, ColumnDef, ColumnMap, Label, ModernTable, RowRef, Table,
    TableAccessor,
};

use super::{builder::LegacyTableBuilder, util::EnumId, FormatConvertError, TableInner};

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
/// fn get_character_id(table: &LegacyTable, row_id: u16) -> u32 {
///     let cell = table.row(row_id).get(Label::from("CharacterID"));
///     // Unlike modern tables, we can't simply operate on the value.
///     // We can `match` on cell types, or simply cast them and handle errors:
///     cell.as_single().unwrap().get_as::<u32>()
/// }
/// ```
#[derive(Debug, Clone, PartialEq)]
pub struct LegacyTable<'b> {
    pub(crate) name: Label,
    pub(crate) base_id: u16,
    pub(crate) columns: ColumnMap,
    pub(crate) rows: Vec<LegacyRow<'b>>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LegacyRow<'b> {
    pub(crate) cells: Vec<Cell<'b>>,
}

impl<'b> LegacyTable<'b> {
    pub(crate) fn new(builder: LegacyTableBuilder<'b>) -> Self {
        Self {
            name: builder.name,
            columns: builder.columns,
            base_id: builder.base_id.try_into().unwrap(), // TODO move to builder
            rows: builder.rows,
        }
    }

    /// Gets an iterator that visits this table's rows
    pub fn rows(&self) -> impl Iterator<Item = RowRef<'_, &LegacyRow<'b>>> {
        self.rows
            .iter()
            .enum_id(self.base_id as u32)
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
    pub fn rows_mut(&mut self) -> impl Iterator<Item = RowRef<'_, &mut LegacyRow<'b>>> {
        self.rows
            .iter_mut()
            .enum_id(self.base_id as u32)
            .map(|(id, row)| RowRef::new(id, row, &self.columns))
    }

    /// Gets an owning iterator over this table's rows
    pub fn into_rows(self) -> impl Iterator<Item = LegacyRow<'b>> {
        self.rows.into_iter()
    }

    /// Gets an owning iterator over this table's rows, in pairs of
    /// `(row ID, row)`.
    pub fn into_rows_id(self) -> impl Iterator<Item = (u16, LegacyRow<'b>)> {
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

    pub(crate) fn check_id(id: u32) -> u16 {
        id.try_into().expect("invalid id for legacy row")
    }
}

impl<'b> LegacyRow<'b> {
    pub fn new(cells: Vec<Cell<'b>>) -> Self {
        Self { cells }
    }

    pub fn cells(&self) -> impl Iterator<Item = &Cell<'b>> {
        self.cells.iter()
    }

    pub fn into_cells(self) -> impl Iterator<Item = Cell<'b>> {
        self.cells.into_iter()
    }
}

impl<'t, 'b: 't> TableAccessor<'t, 'b> for LegacyTable<'b> {
    type Row = &'t LegacyRow<'b>;
    type RowMut = &'t mut LegacyRow<'b>;
    type RowId = u16;

    fn name(&self) -> &Label {
        &self.name
    }

    fn set_name(&mut self, name: Label) {
        self.name = name;
    }

    fn base_id(&self) -> Self::RowId {
        self.base_id
    }

    fn get_row(&'t self, id: Self::RowId) -> Option<RowRef<'t, Self::Row>> {
        let index = id.checked_sub(self.base_id)?;
        self.rows
            .get(index as usize)
            .map(|row| RowRef::new(id as u32, row, &self.columns))
    }

    fn get_row_mut(&'t mut self, id: Self::RowId) -> Option<RowRef<'_, Self::RowMut>> {
        let index = id.checked_sub(self.base_id)?;
        self.rows
            .get_mut(index as usize)
            .map(|row| RowRef::new(id as u32, row, &self.columns))
    }

    fn row_count(&self) -> usize {
        self.rows.len()
    }

    fn column_count(&self) -> usize {
        self.columns.as_slice().len()
    }
}

impl<'a, 'b> CellAccessor for &'a LegacyRow<'b> {
    type Target = &'a Cell<'b>;

    fn access(self, pos: usize) -> Option<Self::Target> {
        self.cells.get(pos)
    }
}

impl<'a, 'b> CellAccessor for &'a mut LegacyRow<'b> {
    type Target = &'a mut Cell<'b>;

    fn access(self, pos: usize) -> Option<Self::Target> {
        self.cells.get_mut(pos)
    }
}

impl<'b> From<LegacyTable<'b>> for LegacyTableBuilder<'b> {
    fn from(value: LegacyTable<'b>) -> Self {
        Self::from_table(value.name, value.base_id as u32, value.columns, value.rows)
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
        //Ok(LegacyTable::new(TableBuilder::from(value)))
        todo!()
    }
}
