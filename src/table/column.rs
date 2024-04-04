use super::private::Column;
use crate::Utf;

/// Hosts both the table's column definitions and an index
/// table to look up cells by column name.
#[derive(Debug, Clone, PartialEq)]
#[doc(hidden)]
pub struct ColumnMap<C: Column, L = <C as Column>::Name> {
    columns: Vec<C>,
    pub(crate) label_map: NameMap<L>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NameMap<L> {
    positions: Vec<(L, usize)>,
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

impl<C: Column> ColumnMap<C, C::Name> {
    pub(crate) fn push(&mut self, column: C) {
        self.label_map.push(column.clone_label());
        self.columns.push(column);
    }

    pub(crate) fn as_slice(&self) -> &[C] {
        &self.columns
    }

    pub(crate) fn as_mut_slice(&mut self) -> &mut [C] {
        &mut self.columns
    }

    pub(crate) fn into_raw(self) -> Vec<C> {
        self.columns
    }

    pub(crate) fn iter(&self) -> impl Iterator<Item = &C> {
        self.columns.iter()
    }
}

impl<L> NameMap<L>
where
    L: PartialEq + Ord,
{
    pub fn position(&self, label: &L) -> Option<usize> {
        self.positions
            .binary_search_by_key(&label, |(l, _)| l)
            .ok()
            .map(|i| self.positions[i].1)
    }

    pub fn push(&mut self, label: L) {
        if let Err(idx) = self.positions.binary_search_by_key(&&label, |(l, _)| l) {
            self.positions.insert(idx, (label, self.positions.len()));
        }
    }
}

impl<C: Column, L> IntoIterator for ColumnMap<C, L> {
    type Item = C;
    type IntoIter = std::vec::IntoIter<C>;

    fn into_iter(self) -> Self::IntoIter {
        self.columns.into_iter()
    }
}

impl<L> FromIterator<L> for NameMap<L>
where
    L: Ord,
{
    fn from_iter<T: IntoIterator<Item = L>>(iter: T) -> Self {
        let mut map = NameMap::default();
        for label in iter {
            map.push(label);
        }
        map
    }
}

impl<C: Column> FromIterator<C> for ColumnMap<C, C::Name> {
    fn from_iter<T: IntoIterator<Item = C>>(iter: T) -> Self {
        let columns: Vec<_> = iter.into_iter().collect();
        Self {
            label_map: columns.iter().map(C::clone_label).collect(),
            columns,
        }
    }
}

impl<L> Default for NameMap<L> {
    fn default() -> Self {
        Self {
            positions: Default::default(),
        }
    }
}

impl<C: Column, L> Default for ColumnMap<C, L> {
    fn default() -> Self {
        Self {
            columns: Default::default(),
            label_map: Default::default(),
        }
    }
}
