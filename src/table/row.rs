use crate::{Label, ColumnMap};

use std::borrow::Borrow;
use std::marker::PhantomData;
use std::ops::{Deref, DerefMut};

/// Best-fit type for row IDs.
/// In legacy BDATs, row identifiers are 16-bit.
/// In modern BDATs, row IDs are 32-bit.
pub type RowId = u32;

/// A reference to a row that also keeps information about the parent table.
///
/// ## Accessing cells
/// Accessing cells from a `RowRef` is very easy:
///
/// ```
/// use bdat::RowRef;
///
/// fn param_1(row: RowRef) -> u32 {
///     // Use .get() to access cells
///     row.get(&"Param1".into()).as_single().unwrap().to_integer()
/// }
///
/// fn param_2_if_present(row: RowRef) -> Option<u32> {
///     // Or use .get_if_present() for columns that might be absent
///     row.get_if_present(&"Param2".into())
///         .and_then(|cell| cell.as_single())
///         .map(|value| value.to_integer())
/// }
/// ```
#[derive(Clone, Copy, Debug)]
pub struct RowRef<'t, R> {
    id: RowId,
    row: R,
    columns: &'t ColumnMap,
}

#[derive(Debug)]
pub struct RowRefMut<'t, 'tbuf, R: 'tbuf> {
    id: RowId,
    row: &'t mut R,
    columns: &'t ColumnMap,
    _buf: PhantomData<&'tbuf ()>,
}

pub trait CellAccessor {
    type Target;

    fn access(&self, pos: usize) -> Option<Self::Target>;
}

impl<'t, R> RowRef<'t, R>
{
    pub(crate) fn new(id: RowId, row: R, columns: &'t ColumnMap) -> Self {
        Self {
            id,
            row,
            columns,
        }
    }

    pub(crate) fn map<O>(self, mapper: impl FnOnce(R) -> O) -> RowRef<'t, O> {
        RowRef { id: self.id, row: mapper(self.row), columns: self.columns }
    }

    pub fn id(&self) -> RowId {
        self.id
    }
}

impl<'t, R> RowRef<'t, R>
where
    R: CellAccessor,
{
    /// Returns a reference to the cell at the given column.
    ///
    /// If there is no column with the given label, this returns [`None`].
    pub fn get_if_present(&self, column: impl Borrow<Label>) -> Option<R::Target> {
        let index = self.columns.position(column.borrow())?;
        self.row.access(index)
    }

    /// Returns a reference to the cell at the given column.
    ///
    /// ## Panics
    /// Panics if there is no column with the given label.
    pub fn get(&self, column: impl Borrow<Label>) -> R::Target {
        self.get_if_present(column).expect("no such column")
    }
}

impl<'a, 't: 'a, 'tbuf, R: 'tbuf> RowRefMut<'t, 'tbuf, R> {
    pub(crate) fn new(id: RowId, row: &'t mut R, columns: &'t ColumnMap) -> Self {
        Self { id, row, columns, _buf: PhantomData }
    }
}

impl<'a, 't: 'a, 'tbuf, R: 'tbuf> RowRefMut<'t, 'tbuf, R> where R: CellAccessor {
    /// Returns a reference to the cell at the given column.
    pub fn get(&'t self, column: &'a Label) -> Option<R::Target> {
        let index = self.columns.position(column)?;
        self.row.access(index)
    }
}


impl<'t, R> Deref for RowRef<'t, R> {
    type Target = R;

    fn deref(&self) -> &Self::Target {
        &self.row
    }
}

impl<'t, R> DerefMut for RowRef<'t, R> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.row
    }
}

impl<'t, 'tbuf, R: 'tbuf> Deref for RowRefMut<'t, 'tbuf, R> {
    type Target = R;

    fn deref(&self) -> &Self::Target {
        self.row
    }
}

impl<'t, 'tbuf, R: 'tbuf> DerefMut for RowRefMut<'t, 'tbuf, R> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.row
    }
}


