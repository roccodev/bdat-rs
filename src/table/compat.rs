use crate::{
    BdatResult, Cell, LegacyTable, ModernTable, ColumnDef, Label, RowRef, RowRefMut, Table, TableAccessor, RowId, ColumnMap, CellAccessor
};
use super::TableInner;
use super::legacy::LegacyRow;
use super::modern::ModernRow;
use super::util::{self, VersionedIter};

pub enum CompatRow<'buf> {
    Modern(ModernRow<'buf>),
    Legacy(LegacyRow<'buf>)
}

pub enum CompatRef<'t, 'buf> {
    Modern(&'t ModernRow<'buf>),
    Legacy(&'t LegacyRow<'buf>)
}

pub enum CompatRefMut<'t, 'buf> {
    Modern(&'t mut ModernRow<'buf>),
    Legacy(&'t mut LegacyRow<'buf>)
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
    ///
    /// This is not to be confused with [`into_modern`], which panics if
    /// the table is not modern.
    ///
    /// [`into_modern`]: Table::into_modern
    pub fn to_modern(self) -> BdatResult<ModernTable<'b>> {
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
    ///
    /// This is not to be confused with [`into_legacy`], which panics if
    /// the table is not legacy.
    ///
    /// [`into_legacy`]: Table::into_legacy
    pub fn to_legacy(self) -> BdatResult<LegacyTable<'b>> {
        match self.inner {
            TableInner::Modern(m) => Ok(m.try_into()?),
            TableInner::Legacy(l) => Ok(l),
        }
    }

    /// Gets an iterator that visits this table's rows
    pub fn rows(&self) -> impl Iterator<Item = RowRef<'_, CompatRef<'_, 'b>>> {
        match &self.inner {
            TableInner::Modern(m) => VersionedIter::Modern(m.rows().map(|r| r.map(CompatRef::Modern))),
            TableInner::Legacy(l) => VersionedIter::Legacy(l.rows().map(|r| r.map(CompatRef::Legacy))),
        }
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
    /// hashed ID (for modern BDATs). Failure to do so will lead to improper behavior of
    /// [`get_row_by_hash`].
    ///
    /// [`get_row_by_hash`]: ModernTable::get_row_by_hash
    pub fn rows_mut(&mut self) -> impl Iterator<Item = RowRef<'_, CompatRefMut<'_, 'b>>> {
        match &mut self.inner {
            TableInner::Modern(m) => VersionedIter::Modern(m.rows_mut().map(|r| r.map(CompatRefMut::Modern))),
            TableInner::Legacy(l) => VersionedIter::Legacy(l.rows_mut().map(|r| r.map(CompatRefMut::Legacy))),
        }
    }

    /// Gets an owning iterator over this table's rows
    pub fn into_rows(self) -> impl Iterator<Item = CompatRow<'b>> {
        match self.inner {
            TableInner::Modern(m) => VersionedIter::Modern(m.into_rows().map(CompatRow::Modern)),
            TableInner::Legacy(l) => VersionedIter::Legacy(l.into_rows().map(CompatRow::Legacy)),
        }
    }

    /// Gets an iterator that visits this table's column definitions
    pub fn columns(&self) -> impl Iterator<Item = &ColumnDef> {
        versioned_iter!(&self.inner, columns())
    }

    /// Gets an iterator over mutable references to this table's
    /// column definitions.
    pub fn columns_mut(&mut self) -> impl Iterator<Item = &mut ColumnDef> {
        versioned_iter!(&mut self.inner, columns_mut())
    }

    /// Gets an owning iterator over this table's column definitions
    pub fn into_columns(self) -> impl Iterator<Item = ColumnDef> {
        versioned_iter!(self.inner, into_columns())
    }
}

impl<'b> CompatRow<'b> {
    pub fn to_modern(self) -> ModernRow<'b> {
        todo!()
    }

    pub fn to_legacy(self) -> LegacyRow<'b> {
        todo!()
    }
}

impl<'t, 'b> CellAccessor for CompatRef<'t, 'b> {
    type Target = &'t Cell<'b>;

    fn access(&self, pos: usize) -> Option<Self::Target> {
        todo!();
    }
}

impl<'t, 'b: 't> TableAccessor<'t, 'b> for Table<'b> {
    type Row = CompatRef<'t, 'b>;
    type RowMut = CompatRefMut<'t, 'b>;
    type RowId = u32;

    fn name(&self) -> &Label {
        versioned!(&self.inner, name)
    }

    fn set_name(&mut self, name: Label) {
        versioned!(&mut self.inner, set_name(name))
    }

    fn base_id(&self) -> Self::RowId {
        match &self.inner {
            TableInner::Modern(m) => m.base_id(),
            TableInner::Legacy(l) => l.base_id() as u32,
        }
    }

    fn row(&'t self, id: Self::RowId) -> RowRef<'t, Self::Row> {
        match &self.inner {
            TableInner::Modern(m) => m.row(id).map(CompatRef::Modern),
            TableInner::Legacy(l) => l.row(LegacyTable::check_id(id)).map(CompatRef::Legacy),
        }
    }

    fn row_mut(&mut self, id: Self::RowId) -> RowRef<'_, Self::RowMut> {
        todo!();
        //versioned_id!(&mut self.inner, row_mut(id))
    }

    fn get_row(&'t self, id: Self::RowId) -> Option<RowRef<'t, Self::Row>> {
        match &self.inner {
            TableInner::Modern(m) => m.get_row(id).map(|r| r.map(CompatRef::Modern)),
            // TODO use option on id fail
            TableInner::Legacy(l) => l.get_row(LegacyTable::check_id(id)).map(|r| r.map(CompatRef::Legacy)),
        }
    }

    fn get_row_mut(&mut self, id: Self::RowId) -> Option<RowRef<'_, Self::RowMut>> {
        todo!();
        //versioned_id!(&mut self.inner, get_row_mut(id))
    }

    fn row_count(&self) -> usize {
        versioned!(&self.inner, row_count())
    }

    fn column_count(&self) -> usize {
        versioned!(&self.inner, column_count())
    }
}

