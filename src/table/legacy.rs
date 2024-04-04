//! Legacy (XC1 up to DE) format types

use crate::{compat::CompatTable, Cell, RowRef, Utf, ValueType};

use super::{
    builder::LegacyTableBuilder,
    column::ColumnMap,
    private::{CellAccessor, Column, ColumnSerialize, LabelMap, Table},
    util::EnumId,
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
/// See also: [`LegacyRow`]
///
/// # Examples
///
/// ## Operating on cells
///
/// ```
/// use bdat::{Label, legacy::LegacyTable, label_hash};
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

/// A row from a legacy BDAT table.
#[derive(Debug, Clone, PartialEq)]
pub struct LegacyRow<'b> {
    pub(crate) cells: Vec<Cell<'b>>,
}

/// A column definition from a legacy BDAT table
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LegacyColumn<'buf> {
    pub(crate) value_type: ValueType,
    pub(crate) label: Utf<'buf>,
    pub(crate) count: usize,
    pub(crate) flags: Vec<LegacyFlag<'buf>>,
}

/// A sub-definition for flag data that is associated to a column in legacy formats.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct LegacyFlag<'tb> {
    /// The flag's identifier. Because flags are only supported in legacy BDATs, this is
    /// equivalent to a [`Label::String`].
    pub(crate) label: Utf<'tb>,
    /// The bits this flag is setting on the parent
    pub(crate) mask: u32,
    /// The index in the parent cell's flag list
    #[cfg_attr(feature = "serde", serde(rename = "index"))]
    pub(crate) flag_index: usize,
}

/// A builder interface for [`LegacyColumn`].
pub struct LegacyColumnBuilder<'tb>(LegacyColumn<'tb>);

/// The [`RowRef`] returned by queries on [`LegacyTable`].
pub type LegacyRowRef<'t, 'buf> = RowRef<&'t LegacyRow<'buf>, &'t ColumnMap<LegacyColumn<'buf>>>;
/// The [`RowRef`] (mutable view) returned by queries on [`LegacyTable`].
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

impl<'tb> LegacyColumn<'tb> {
    /// Creates a new [`LegacyColumn`]. For more advanced settings, such as item count or flag
    /// data, use [`LegacyColumnBuilder`].
    pub fn new(ty: ValueType, label: Utf<'tb>) -> Self {
        Self::with_flags(ty, label, Vec::new())
    }

    fn with_flags(ty: ValueType, label: Utf<'tb>, flags: Vec<LegacyFlag<'tb>>) -> Self {
        Self {
            value_type: ty,
            label,
            flags,
            count: 1,
        }
    }

    /// Returns this column's type.
    pub fn value_type(&self) -> ValueType {
        self.value_type
    }

    /// Returns this column's name.
    pub fn label(&self) -> &str {
        self.label.as_ref()
    }

    /// Returns the number of values in this column's cells.
    /// For [`Cell::Single`] and [`Cell::Flags`] cells, this is 1. For [`Cell::List`] cells, it is
    /// the number of elements in the list.
    ///
    /// [`Cell::Single`]: crate::Cell::Single
    /// [`Cell::Flags`]: crate::Cell::Flags
    /// [`Cell::List`]: crate::Cell::List
    pub fn count(&self) -> usize {
        self.count
    }

    /// Returns this column's defined set of sub-flags.
    pub fn flags(&self) -> &[LegacyFlag<'tb>] {
        &self.flags
    }

    /// Returns the total space occupied by a cell of this column.
    pub fn data_size(&self) -> usize {
        self.value_type.data_len() * self.count
    }
}

impl<'tb> LegacyFlag<'tb> {
    /// Creates a flag definition with an arbitrary mask and shift amount.
    pub fn new(label: impl Into<Utf<'tb>>, mask: u32, shift_amount: usize) -> Self {
        Self {
            label: label.into(),
            mask,
            flag_index: shift_amount,
        }
    }

    /// Creates a flag definition that only masks a single bit.
    ///
    /// Bits are numbered starting from 0, i.e. the least significant bit of the parent value
    /// is the bit at index 0.
    ///
    /// Note: the bit must not be greater than the parent value's bit count.
    /// For example, a bit of 14 is invalid for an 8-bit value.
    pub fn new_bit(label: impl Into<Utf<'tb>>, bit: u32) -> Self {
        Self::new(label, 1 << bit, bit as usize)
    }

    /// Returns this flag's name.
    pub fn label(&self) -> &str {
        &self.label
    }

    /// Returns this flag's bit mask.
    pub fn mask(&self) -> u32 {
        self.mask
    }

    /// Returns this flag's right shift amount.
    pub fn shift_amount(&self) -> usize {
        self.flag_index
    }
}

impl<'tb> LegacyColumnBuilder<'tb> {
    pub fn new(value_type: ValueType, label: Utf<'tb>) -> Self {
        Self(LegacyColumn::new(value_type, label))
    }

    /// Sets the column's full flag data.
    pub fn set_flags(mut self, flags: Vec<LegacyFlag<'tb>>) -> Self {
        self.0.flags = flags;
        self
    }

    /// Sets how many elements the column holds, if cells are of the list type.
    pub fn set_count(mut self, count: usize) -> Self {
        assert!(count > 0);
        self.0.count = count;
        self
    }

    pub fn build(self) -> LegacyColumn<'tb> {
        self.0
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
        Self::from_table(value.name, value.base_id, value.columns, value.rows)
    }
}

impl<'b> From<LegacyTable<'b>> for CompatTable<'b> {
    fn from(value: LegacyTable<'b>) -> Self {
        Self::Legacy(value)
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

    fn ser_flags(&self) -> &[LegacyFlag] {
        &self.flags
    }
}

impl<'buf> Column for LegacyColumn<'buf> {
    type Name = Utf<'buf>;

    fn value_type(&self) -> ValueType {
        self.value_type
    }

    fn clone_label(&self) -> Self::Name {
        self.label.clone()
    }
}

impl<'tb> From<LegacyColumn<'tb>> for LegacyColumnBuilder<'tb> {
    fn from(value: LegacyColumn<'tb>) -> Self {
        Self(value)
    }
}

impl<'tb> From<LegacyColumnBuilder<'tb>> for LegacyColumn<'tb> {
    fn from(value: LegacyColumnBuilder<'tb>) -> Self {
        value.build()
    }
}
