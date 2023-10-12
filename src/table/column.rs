use crate::{Label, ValueType};

/// A column definition from a Bdat table
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ColumnDef {
    pub(crate) value_type: ValueType,
    pub(crate) label: Label,
    pub(crate) count: usize,
    pub(crate) flags: Vec<FlagDef>,
}

/// A builder interface for [`ColumnDef`].
pub struct ColumnBuilder(ColumnDef);

#[derive(Debug, Clone, PartialEq, Default)]
pub(crate) struct ColumnMap {
    pub columns: Vec<ColumnDef>,
}

/// A sub-definition for flag data that is associated to a column
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct FlagDef {
    /// The flag's identifier. Because flags are only supported in legacy BDATs, this is
    /// equivalent to a [`Label::String`].
    pub(crate) label: String,
    /// The bits this flag is setting on the parent
    pub(crate) mask: u32,
    /// The index in the parent cell's flag list
    #[cfg_attr(feature = "serde", serde(rename = "index"))]
    pub(crate) flag_index: usize,
}

impl ColumnDef {
    /// Creates a new [`ColumnDef`]. For more advanced settings, such as item count or flag
    /// data, use [`ColumnBuilder`].
    pub fn new(ty: ValueType, label: Label) -> Self {
        Self::with_flags(ty, label, Vec::new())
    }

    fn with_flags(ty: ValueType, label: Label, flags: Vec<FlagDef>) -> Self {
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
    pub fn label(&self) -> &Label {
        &self.label
    }

    /// Returns a mutable reference to this column's name.
    pub fn label_mut(&mut self) -> &mut Label {
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
    pub fn flags(&self) -> &[FlagDef] {
        &self.flags
    }

    /// Returns the total space occupied by a cell of this column.
    pub fn data_size(&self) -> usize {
        self.value_type.data_len() * self.count
    }
}

impl FlagDef {
    /// Creates a flag definition with an arbitrary mask and shift amount.
    pub fn new(label: impl Into<String>, mask: u32, shift_amount: usize) -> Self {
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
    pub fn new_bit(label: impl Into<String>, bit: u32) -> Self {
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

impl ColumnBuilder {
    pub fn new(value_type: ValueType, label: Label) -> Self {
        Self(ColumnDef::new(value_type, label))
    }

    /// Sets the column's full flag data.
    pub fn set_flags(mut self, flags: Vec<FlagDef>) -> Self {
        self.0.flags = flags;
        self
    }

    /// Sets how many elements the column holds, if cells are of the list type.
    pub fn set_count(mut self, count: usize) -> Self {
        assert!(count > 0);
        self.0.count = count;
        self
    }

    pub fn build(self) -> ColumnDef {
        self.0
    }
}

impl ColumnMap {
    pub fn position(&self, label: &Label) -> Option<usize> {
        self.columns.iter().position(|c| &c.label == label)
    }

    pub fn push(&mut self, column: ColumnDef) {
        self.columns.push(column);
    }

    pub fn as_slice(&self) -> &[ColumnDef] {
        &self.columns
    }

    pub fn as_mut_slice(&mut self) -> &mut [ColumnDef] {
        &mut self.columns
    }

    pub fn into_raw(self) -> Vec<ColumnDef> {
        self.columns
    }
}

impl<T> From<T> for ColumnMap
where
    T: IntoIterator<Item = ColumnDef>,
{
    fn from(value: T) -> Self {
        Self {
            columns: value.into_iter().collect(),
        }
    }
}
