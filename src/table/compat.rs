//! Adapters for legacy<->modern BDAT compatibility.

use super::legacy::LegacyRow;
use super::modern::ModernRow;
use super::util::{self, VersionedIter};
use super::TableInner;
use crate::{
    BdatResult, Cell, CellAccessor, ColumnDef, Label, LegacyTable, ModernTable, RowId, RowRef,
    Table,
};

pub enum CompatRow<'buf> {
    Modern(ModernRow<'buf>),
    Legacy(LegacyRow<'buf>),
}

#[derive(Clone, Copy)]
pub enum CompatRef<'t, 'buf> {
    Modern(&'t ModernRow<'buf>),
    Legacy(&'t LegacyRow<'buf>),
}

macro_rules! versioned {
    ($var:expr, $name:ident) => {
        match $var {
            TableInner::Modern(m) => &m.$name,
            TableInner::Legacy(l) => &l.$name,
        }
    };
    ($var:expr, $name:ident($($par:expr ) *)) => {
        match $var {
            TableInner::Modern(m) => m . $name ( $($par, )* ),
            TableInner::Legacy(l) => l . $name ( $($par, )* ),
        }
    };
}

macro_rules! versioned_iter {
    ($var:expr, $name:ident($($par:expr ) *)) => {
        match $var {
            TableInner::Modern(m) => util::VersionedIter::Modern(m . $name ( $($par, )* )),
            TableInner::Legacy(l) => util::VersionedIter::Legacy(l . $name ( $($par, )* )),
        }
    };
}

impl<'b> Table<'b> {
    /// If the table is modern, returns a view of the underlying table.
    ///
    /// ## Panics
    /// Panics if the table is not modern.
    pub fn as_modern(&self) -> &ModernTable<'b> {
        match &self.inner {
            TableInner::Modern(m) => m,
            _ => panic!("not modern"),
        }
    }

    /// If the table is legacy, returns a view of the underlying table.
    ///
    /// ## Panics
    /// Panics if the table is not legacy.
    pub fn as_legacy(&self) -> &LegacyTable<'b> {
        match &self.inner {
            TableInner::Legacy(l) => l,
            _ => panic!("not legacy"),
        }
    }

    /// If the table is modern, returns a mutable view of the underlying table.
    ///
    /// ## Panics
    /// Panics if the table is not modern.
    pub fn as_modern_mut(&mut self) -> &mut ModernTable<'b> {
        match &mut self.inner {
            TableInner::Modern(m) => m,
            _ => panic!("not modern"),
        }
    }

    /// If the table is legacy, returns a mutable view of the underlying table.
    ///
    /// ## Panics
    /// Panics if the table is not legacy.
    pub fn as_legacy_mut(&mut self) -> &mut LegacyTable<'b> {
        match &mut self.inner {
            TableInner::Legacy(l) => l,
            _ => panic!("not legacy"),
        }
    }

    /// If the table is modern, returns the underlying table.
    ///
    /// ## Panics
    /// Panics if the table is not modern.  
    /// For a panic-free function that converts instead, use [`to_modern`].
    ///
    /// [`to_modern`]: Table::to_modern
    pub fn into_modern(self) -> ModernTable<'b> {
        match self.inner {
            TableInner::Modern(m) => m,
            _ => panic!("not modern"),
        }
    }

    /// If the table is legacy, returns the underlying table.
    ///
    /// ## Panics
    /// Panics if the table is not legacy.  
    /// For a panic-free function that converts instead, use [`to_legacy`].
    ///
    /// [`to_legacy`]: Table::to_legacy
    pub fn into_legacy(self) -> LegacyTable<'b> {
        match self.inner {
            TableInner::Legacy(l) => l,
            _ => panic!("not legacy"),
        }
    }

    /// Returns whether the underlying table is modern.
    pub fn is_modern(&self) -> bool {
        matches!(self.inner, TableInner::Modern(_))
    }

    /// Returns whether the underlying table is legacy.
    pub fn is_legacy(&self) -> bool {
        matches!(self.inner, TableInner::Legacy(_))
    }

    /// Returns a modern table as close to the underlying table as possible.
    ///
    /// * If the table is modern, this does nothing and returns it.
    /// * If the table is legacy, it tries to convert it to the
    /// modern format, and returns the result.
    pub fn try_into_modern(self) -> BdatResult<ModernTable<'b>> {
        match self.inner {
            TableInner::Modern(m) => Ok(m),
            TableInner::Legacy(l) => Ok(l.try_into()?),
        }
    }

    /// Returns a legacy table as close to the underlying table as possible.
    ///
    /// * If the table is legacy, this does nothing and returns it.
    /// * If the table is modern, it tries to convert it to the
    /// legacy format, and returns the result.
    pub fn try_into_legacy(self) -> BdatResult<LegacyTable<'b>> {
        match self.inner {
            TableInner::Modern(m) => Ok(m.try_into()?),
            TableInner::Legacy(l) => Ok(l),
        }
    }

    pub fn name(&self) -> Label {
        match &self.inner {
            TableInner::Modern(m) => m.name(),
            TableInner::Legacy(l) => l.name().into(),
        }
    }

    pub fn set_name(&mut self, name: Label<'b>) {
        match &mut self.inner {
            TableInner::Modern(m) => m.set_name(name),
            TableInner::Legacy(l) => {
                l.set_name(name.try_into().expect("hashed labels are not supported"))
            }
        }
    }

    /// Gets the minimum row ID in the table.
    pub fn base_id(&self) -> RowId {
        match &self.inner {
            TableInner::Modern(m) => m.base_id(),
            TableInner::Legacy(l) => l.base_id() as u32,
        }
    }

    /// Gets a row by its ID.
    ///
    /// Note: the ID is the row's numerical ID, which could be different
    /// from the index of the row in the table's row list. That is because
    /// BDAT tables can have arbitrary start IDs.
    ///
    /// ## Panics
    /// If there is no row for the given ID.
    pub fn row(&self, id: RowId) -> RowRef<'_, CompatRef<'_, 'b>> {
        match &self.inner {
            TableInner::Modern(m) => m.row(id).map(CompatRef::Modern),
            TableInner::Legacy(l) => l
                .row(id.try_into().expect("invalid id for legacy row"))
                .map(CompatRef::Legacy),
        }
    }

    /// Attempts to get a row by its ID.  
    /// If there is no row for the given ID, this returns [`None`].
    ///
    /// Note: the ID is the row's numerical ID, which could be different
    /// from the index of the row in the table's row list. That is because
    /// BDAT tables can have arbitrary start IDs.
    pub fn get_row(&self, id: RowId) -> Option<RowRef<'_, CompatRef<'_, 'b>>> {
        match &self.inner {
            TableInner::Modern(m) => m.get_row(id).map(|r| r.map(CompatRef::Modern)),
            TableInner::Legacy(l) => id
                .try_into()
                .ok()
                .and_then(|id| l.get_row(id))
                .map(|r| r.map(CompatRef::Legacy)),
        }
    }

    /// Gets an iterator that visits this table's rows
    pub fn rows(&self) -> impl Iterator<Item = RowRef<'_, CompatRef<'_, 'b>>> {
        match &self.inner {
            TableInner::Modern(m) => {
                VersionedIter::Modern(m.rows().map(|r| r.map(CompatRef::Modern)))
            }
            TableInner::Legacy(l) => {
                VersionedIter::Legacy(l.rows().map(|r| r.map(CompatRef::Legacy)))
            }
        }
    }

    /// Gets an owning iterator over this table's rows
    pub fn into_rows(self) -> impl Iterator<Item = CompatRow<'b>> {
        match self.inner {
            TableInner::Modern(m) => VersionedIter::Modern(m.into_rows().map(CompatRow::Modern)),
            TableInner::Legacy(l) => VersionedIter::Legacy(l.into_rows().map(CompatRow::Legacy)),
        }
    }

    /// Gets an owning iterator over this table's rows, in pairs of
    /// `(row ID, row)`.
    pub fn into_rows_id(self) -> impl Iterator<Item = (u32, CompatRow<'b>)> {
        match self.inner {
            TableInner::Modern(m) => {
                VersionedIter::Modern(m.into_rows_id().map(|(id, r)| (id, CompatRow::Modern(r))))
            }
            TableInner::Legacy(l) => VersionedIter::Legacy(
                l.into_rows_id()
                    .map(|(id, r)| (id as u32, CompatRow::Legacy(r))),
            ),
        }
    }

    /// Gets an iterator that visits this table's column definitions
    pub fn columns(&self) -> impl Iterator<Item = &ColumnDef> {
        versioned_iter!(&self.inner, columns())
    }

    /// Gets an owning iterator over this table's column definitions
    pub fn into_columns(self) -> impl Iterator<Item = ColumnDef<'b>> {
        versioned_iter!(self.inner, into_columns())
    }

    pub fn row_count(&self) -> usize {
        versioned!(&self.inner, row_count())
    }

    pub fn column_count(&self) -> usize {
        versioned!(&self.inner, column_count())
    }
}

impl<'b> CompatRow<'b> {
    pub fn cells(&self) -> impl Iterator<Item = Cell<'b>> + '_ {
        match self {
            CompatRow::Modern(m) => {
                VersionedIter::Modern(m.values.iter().map(|v| Cell::Single(v.clone())))
            }
            CompatRow::Legacy(l) => VersionedIter::Legacy(l.cells.iter().cloned()),
        }
    }

    pub fn into_cells(self) -> impl Iterator<Item = Cell<'b>> {
        match self {
            CompatRow::Modern(m) => VersionedIter::Modern(m.into_values().map(Cell::Single)),
            CompatRow::Legacy(l) => VersionedIter::Legacy(l.into_cells()),
        }
    }
}

impl<'t, 'b> CompatRef<'t, 'b> {
    pub fn cells(&self) -> impl Iterator<Item = Cell<'b>> + '_ {
        match self {
            CompatRef::Modern(m) => {
                VersionedIter::Modern(m.values.iter().map(|v| Cell::Single(v.clone())))
            }
            CompatRef::Legacy(l) => VersionedIter::Legacy(l.cells.iter().cloned()),
        }
    }
}

impl<'t, 'b> CellAccessor for CompatRef<'t, 'b> {
    type Target = Cell<'b>;

    fn access(self, pos: usize) -> Option<Self::Target> {
        match self {
            CompatRef::Modern(m) => m.values.get(pos).map(|v| Cell::Single(v.clone())),
            CompatRef::Legacy(l) => l.cells.get(pos).cloned(),
        }
    }
}
