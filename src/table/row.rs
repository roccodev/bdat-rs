use crate::{Cell, Label, Value};
use crate::{ColumnMap, Table};
use std::borrow::Borrow;
use std::marker::PhantomData;

use std::ops::{Deref, DerefMut, Index};

/// A row from a Bdat table
#[derive(Debug, Clone, PartialEq)]
pub struct Row<'b> {
    pub(crate) id: usize,
    pub(crate) cells: Vec<Cell<'b>>,
}

/// A reference to a row that also keeps information about the parent table.
///
/// ## Accessing cells
/// Accessing cells from a `RowRef` is very easy:
///
/// ```
/// use bdat::RowRef;
///
/// fn param_1(row: RowRef) -> u32 {
///     // Use the index syntax (or .get()) to access cells
///     row["Param1"].as_single().unwrap().to_integer()
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
pub struct RowRef<'t, 'tb, C = &'t Cell<'tb>>
where
    C: From<&'t Cell<'tb>>,
{
    row: &'t Row<'tb>,
    table: &'t Table<'tb>,
    _cell: PhantomData<C>,
}

#[derive(Debug)]
pub struct RowRefMut<'t, 'tb> {
    row: &'t mut Row<'tb>,
    columns: &'t ColumnMap,
}

impl<'b> Row<'b> {
    /// Creates a new [`Row`].
    pub fn new(id: usize, cells: Vec<Cell<'b>>) -> Self {
        Self { id, cells }
    }

    /// Gets the row's ID
    pub fn id(&self) -> usize {
        self.id
    }

    /// Gets an owning iterator over this row's cells
    pub fn into_cells(self) -> impl Iterator<Item = Cell<'b>> {
        self.cells.into_iter()
    }

    /// Gets an iterator over this row's cells
    pub fn cells(&self) -> impl Iterator<Item = &Cell<'b>> {
        self.cells.iter()
    }

    /// Searches the row's cells for a ID hash field, returning the ID
    /// of this row if found.
    pub fn id_hash(&self) -> Option<u32> {
        self.cells.iter().find_map(|cell| match cell {
            Cell::Single(Value::HashRef(id)) => Some(*id),
            _ => None,
        })
    }
}

impl<'t, 'tb, C> RowRef<'t, 'tb, C>
where
    C: From<&'t Cell<'tb>>,
{
    pub(crate) fn new(table: &'t Table<'tb>, row: &'t Row<'tb>) -> Self {
        Self {
            table,
            row,
            _cell: PhantomData,
        }
    }

    /// Returns a reference to the cell at the given column.
    ///
    /// If there is no column with the given label, this returns [`None`].
    pub fn get_if_present(&self, column: impl Borrow<Label>) -> Option<C> {
        let index = self.table.columns.position(column.borrow())?;
        self.row.cells.get(index).map(Into::into)
    }

    /// Returns a reference to the cell at the given column.
    ///
    /// ## Panics
    /// Panics if there is no column with the given label.
    pub fn get(&self, column: impl Borrow<Label>) -> C {
        self.get_if_present(column).expect("no such column")
    }

    /// Returns the table this row belongs to.
    pub fn table(&self) -> &'t Table<'tb> {
        self.table
    }
}

impl<'t, 'tb> RowRef<'t, 'tb> {
    pub(crate) fn into_with_cell_type<C>(self) -> RowRef<'t, 'tb, C>
    where
        C: From<&'t Cell<'tb>>,
    {
        RowRef {
            row: self.row,
            table: self.table,
            _cell: PhantomData,
        }
    }
}

impl<'a, 't: 'a, 'tb> RowRefMut<'t, 'tb> {
    pub(crate) fn new(row: &'t mut Row<'tb>, columns: &'t ColumnMap) -> Self {
        Self { row, columns }
    }

    /// Returns a reference to the cell at the given column.
    pub fn get(&'t self, column: &'a Label) -> Option<&'t Cell<'tb>> {
        let index = self.columns.position(column)?;
        self.row.cells.get(index)
    }
}

// Implementation for e.g. row["string slice"]
impl<'a, 't: 'a, 'tb, S> Index<S> for RowRef<'t, 'tb>
where
    S: Into<Label> + PartialEq,
{
    type Output = Cell<'tb>;

    fn index(&self, index: S) -> &Self::Output {
        self.get(&index.into())
    }
}

impl<'t, 'tb> Deref for RowRef<'t, 'tb> {
    type Target = Row<'tb>;

    fn deref(&self) -> &Self::Target {
        self.row
    }
}

impl<'t, 'tb> Deref for RowRefMut<'t, 'tb> {
    type Target = Row<'tb>;

    fn deref(&self) -> &Self::Target {
        self.row
    }
}

impl<'t, 'tb> DerefMut for RowRefMut<'t, 'tb> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.row
    }
}
