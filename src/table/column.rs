use crate::{Utf, ValueType};

/// A column definition from a Bdat table
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Column<'tb, L> {
    pub(crate) value_type: ValueType,
    pub(crate) label: L,
    pub(crate) count: usize,
    pub(crate) flags: Vec<FlagDef<'tb>>,
}

/// A builder interface for [`Column`].
pub struct ColumnBuilder<'tb, L>(Column<'tb, L>);

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ColumnMap<'tb, L> {
    pub columns: Vec<Column<'tb, L>>,
}

/// A sub-definition for flag data that is associated to a column
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct FlagDef<'tb> {
    /// The flag's identifier. Because flags are only supported in legacy BDATs, this is
    /// equivalent to a [`Label::String`].
    pub(crate) label: Utf<'tb>,
    /// The bits this flag is setting on the parent
    pub(crate) mask: u32,
    /// The index in the parent cell's flag list
    #[cfg_attr(feature = "serde", serde(rename = "index"))]
    pub(crate) flag_index: usize,
}

impl<'tb, L> Column<'tb, L> {
    /// Creates a new [`Column`]. For more advanced settings, such as item count or flag
    /// data, use [`ColumnBuilder`].
    pub fn new(ty: ValueType, label: L) -> Self {
        Self::with_flags(ty, label, Vec::new())
    }

    fn with_flags(ty: ValueType, label: L, flags: Vec<FlagDef<'tb>>) -> Self {
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
    pub fn label(&self) -> &L {
        &self.label
    }

    /// Returns a mutable reference to this column's name.
    pub fn label_mut(&mut self) -> &mut L {
        &mut self.label
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
    pub fn flags(&self) -> &[FlagDef<'tb>] {
        &self.flags
    }

    /// Returns the total space occupied by a cell of this column.
    pub fn data_size(&self) -> usize {
        self.value_type.data_len() * self.count
    }

    pub(crate) fn map_label<M>(self, map_fn: impl Fn(L) -> M) -> Column<'tb, M> {
        Column {
            label: map_fn(self.label),
            value_type: self.value_type,
            count: self.count,
            flags: self.flags,
        }
    }
}

impl<'tb> FlagDef<'tb> {
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

impl<'tb, L> ColumnBuilder<'tb, L> {
    pub fn new(value_type: ValueType, label: L) -> Self {
        Self(Column::new(value_type, label))
    }

    /// Sets the column's full flag data.
    pub fn set_flags(mut self, flags: Vec<FlagDef<'tb>>) -> Self {
        self.0.flags = flags;
        self
    }

    /// Sets how many elements the column holds, if cells are of the list type.
    pub fn set_count(mut self, count: usize) -> Self {
        assert!(count > 0);
        self.0.count = count;
        self
    }

    pub fn build(self) -> Column<'tb, L> {
        self.0
    }
}

impl<'tb, L> ColumnMap<'tb, L> {
    pub fn position(&self, label: L) -> Option<usize>
    where
        L: PartialEq,
    {
        self.columns.iter().position(|c| c.label == label)
    }

    pub fn push(&mut self, column: Column<'tb, L>) {
        self.columns.push(column);
    }

    pub fn as_slice(&self) -> &[Column<'tb, L>] {
        &self.columns
    }

    pub fn as_mut_slice(&mut self) -> &mut [Column<'tb, L>] {
        &mut self.columns
    }

    pub fn into_raw(self) -> Vec<Column<'tb, L>> {
        self.columns
    }

    pub fn iter(&self) -> impl Iterator<Item = &Column<'tb, L>> {
        self.columns.iter()
    }
}

impl<'tb, L> IntoIterator for ColumnMap<'tb, L> {
    type Item = Column<'tb, L>;
    type IntoIter = std::vec::IntoIter<Column<'tb, L>>;

    fn into_iter(self) -> Self::IntoIter {
        self.columns.into_iter()
    }
}

impl<'tb, L> FromIterator<Column<'tb, L>> for ColumnMap<'tb, L> {
    fn from_iter<T: IntoIterator<Item = Column<'tb, L>>>(iter: T) -> Self {
        Self {
            columns: iter.into_iter().collect(),
        }
    }
}

impl<'tb, L> From<Column<'tb, L>> for ColumnBuilder<'tb, L> {
    fn from(value: Column<'tb, L>) -> Self {
        Self(value)
    }
}

impl<'a, L> Default for ColumnMap<'a, L> {
    fn default() -> Self {
        Self {
            columns: Default::default(),
        }
    }
}
