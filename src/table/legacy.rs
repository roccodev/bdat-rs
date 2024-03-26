use crate::{
    Cell, CellAccessor, ColumnMap, CompatInner, CompatTable, CompatTableBuilder, Label, LabelMap,
    LegacyColumn, ModernTable, RowRef, Utf,
};

use super::{
    builder::LegacyTableBuilder, private::ColumnSerialize, util::EnumId, FormatConvertError, Table,
};

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
/// use bdat::{Label, LegacyTable, label_hash};
///
/// fn get_character_id(table: &LegacyTable, row_id: u16) -> u32 {
///     let cell = table.row(row_id).get("CharacterID");
///     // Unlike modern tables, we can't simply operate on the value.
///     // We can `match` on cell types, or simply cast them and handle errors:
///     cell.as_single().unwrap().get_as::<u32>()
/// }
/// ```
#[derive(Debug, Clone, PartialEq)]
pub struct LegacyTable<'b> {
    pub(crate) name: Utf<'b>,
    pub(crate) base_id: u16,
    // Need to make Utf<'b> explicit here, otherwise the type becomes invariant over 'b
    // (limitation of associated types)
    pub(crate) columns: ColumnMap<LegacyColumn<'b>, Utf<'b>>,
    pub(crate) rows: Vec<LegacyRow<'b>>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LegacyRow<'b> {
    pub(crate) cells: Vec<Cell<'b>>,
}

pub type LegacyRowRef<'t, 'buf> = RowRef<&'t LegacyRow<'buf>, &'t ColumnMap<LegacyColumn<'buf>>>;
pub type LegacyRowMut<'t, 'buf> =
    RowRef<&'t mut LegacyRow<'buf>, &'t ColumnMap<LegacyColumn<'buf>>>;

impl<'b> LegacyTable<'b> {
    pub(crate) fn new(builder: LegacyTableBuilder<'b>) -> Self {
        Self {
            name: builder.name,
            columns: builder.columns,
            base_id: builder.base_id,
            rows: builder.rows,
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn set_name(&mut self, name: Utf<'b>) {
        self.name = name;
    }

    /// Gets the minimum row ID in the table.
    pub fn base_id(&self) -> u16 {
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
    pub fn row(&self, id: u16) -> LegacyRowRef<'_, 'b> {
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
    pub fn row_mut(&mut self, id: u16) -> LegacyRowMut<'_, 'b> {
        self.get_row_mut(id).expect("row not found")
    }

    /// Attempts to get a row by its ID.  
    /// If there is no row for the given ID, this returns [`None`].
    ///
    /// Note: the ID is the row's numerical ID, which could be different
    /// from the index of the row in the table's row list. That is because
    /// BDAT tables can have arbitrary start IDs.
    pub fn get_row(&self, id: u16) -> Option<LegacyRowRef<'_, 'b>> {
        let index = id.checked_sub(self.base_id)?;
        self.rows
            .get(index as usize)
            .map(|row| RowRef::new(id as u32, row, &self.columns))
    }

    /// Attempts to get a mutable view of a row by its ID.  
    /// If there is no row for the given ID, this returns [`None`].
    ///
    /// Note: the ID is the row's numerical ID, which could be different
    /// from the index of the row in the table's row list. That is because
    /// BDAT tables can have arbitrary start IDs.
    pub fn get_row_mut(&mut self, id: u16) -> Option<LegacyRowMut<'_, 'b>> {
        let index = id.checked_sub(self.base_id)?;
        self.rows
            .get_mut(index as usize)
            .map(|row| RowRef::new(id as u32, row, &self.columns))
    }

    /// Gets an iterator that visits this table's rows
    pub fn rows(&self) -> impl Iterator<Item = LegacyRowRef<'_, 'b>> {
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
    pub fn rows_mut(&mut self) -> impl Iterator<Item = LegacyRowMut<'_, 'b>> {
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
    pub fn columns(&self) -> impl Iterator<Item = &LegacyColumn<'b>> {
        self.columns.iter()
    }

    /// Gets an iterator over mutable references to this table's
    /// column definitions.
    pub fn columns_mut(&mut self) -> impl Iterator<Item = &mut LegacyColumn<'b>> {
        self.columns.as_mut_slice().iter_mut()
    }

    /// Gets an owning iterator over this table's column definitions
    pub fn into_columns(self) -> impl Iterator<Item = LegacyColumn<'b>> {
        self.columns.into_raw().into_iter()
    }

    pub fn row_count(&self) -> usize {
        self.rows.len()
    }

    pub fn column_count(&self) -> usize {
        self.columns.as_slice().len()
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

impl<'buf> Table<'buf> for LegacyTable<'buf> {
    type Id = u16;
    type Name = Utf<'buf>;
    type Row = LegacyRow<'buf>;
    type BuilderRow = LegacyRow<'buf>;
    type Column = LegacyColumn<'buf>;
    type BuilderColumn = LegacyColumn<'buf>;
}

impl<'a, 'b> CellAccessor for &'a LegacyRow<'b> {
    type Target = &'a Cell<'b>;
    type ColName<'l> = Utf<'l>;

    fn access(self, pos: usize) -> Option<Self::Target> {
        self.cells.get(pos)
    }
}

impl<'a, 'b> CellAccessor for &'a mut LegacyRow<'b> {
    type Target = &'a mut Cell<'b>;
    type ColName<'l> = Utf<'l>;

    fn access(self, pos: usize) -> Option<Self::Target> {
        self.cells.get_mut(pos)
    }
}

impl<'b> From<LegacyTable<'b>> for LegacyTableBuilder<'b> {
    fn from(value: LegacyTable<'b>) -> Self {
        Self::from_table(value.name, value.base_id, value.columns, value.rows)
    }
}

impl<'b> From<LegacyTable<'b>> for CompatTableBuilder<'b> {
    fn from(value: LegacyTable<'b>) -> Self {
        Self::from(LegacyTableBuilder::from(value))
    }
}

impl<'b> From<LegacyTable<'b>> for CompatTable<'b> {
    fn from(value: LegacyTable<'b>) -> Self {
        Self::from_inner(CompatInner::Legacy(value))
    }
}

impl<'b> TryFrom<ModernTable<'b>> for LegacyTable<'b> {
    type Error = FormatConvertError;

    fn try_from(value: ModernTable<'b>) -> Result<Self, Self::Error> {
        CompatTableBuilder::from(value)
            .try_into_legacy()?
            .try_build()
    }
}

impl<'t, 'b> LabelMap for &'t ColumnMap<LegacyColumn<'b>, Utf<'b>> {
    type Name = Utf<'b>;

    fn position(&self, label: &Self::Name) -> Option<usize> {
        self.label_map.position(label)
    }
}

impl<'buf> ColumnSerialize for LegacyColumn<'buf> {
    fn ser_value_type(&self) -> crate::ValueType {
        self.value_type()
    }

    fn ser_flags(&self) -> &[crate::LegacyFlag] {
        &self.flags
    }
}
