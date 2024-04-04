use crate::{Label, Utf};

use super::{legacy::LegacyColumn, modern::ModernColumn, private::Column};

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

#[derive(Clone, Copy)]
pub enum CompatColumnMap<'t, 'buf> {
    Modern(&'t ColumnMap<ModernColumn<'buf>, Label<'buf>>),
    Legacy(&'t ColumnMap<LegacyColumn<'buf>, Utf<'buf>>),
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
