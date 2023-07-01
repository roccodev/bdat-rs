use crate::Table;
use crate::{Cell, Label, Value};
use std::borrow::Borrow;
use std::ops::Index;

/// A row from a Bdat table
#[derive(Debug, Clone, PartialEq)]
pub struct Row<'b> {
    pub id: usize,
    pub cells: Vec<Cell<'b>>,
}

pub struct RowRef<'t, 'tb> {
    index: usize,
    id: usize,
    table: &'t Table<'tb>,
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

impl<'t, 'tb> RowRef<'t, 'tb> {
    pub(crate) fn new(table: &'t Table<'tb>, index: usize, id: usize) -> Self {
        Self { index, id, table }
    }

    /// Returns the row's original ID
    pub fn id(&self) -> usize {
        self.id
    }

    /// Returns a reference to the cell at the given column.
    pub fn get(&self, column: impl Borrow<Label>) -> Option<&'t Cell<'tb>> {
        let label = column.borrow();
        let index = self
            .table
            .columns
            .iter()
            .position(|col| col.label == *label)?;
        self.table.rows[self.index].cells.get(index)
    }

    /// Returns the table this row belongs to.
    pub fn table(&self) -> &'t Table<'tb> {
        self.table
    }
}

impl<'t, 'tb, S> Index<S> for RowRef<'t, 'tb>
where
    S: Into<Label>,
{
    type Output = Cell<'tb>;

    fn index(&self, index: S) -> &Self::Output {
        let index = index.into();
        let index = self
            .table
            .columns
            .iter()
            .position(|col| col.label == index)
            .expect("no such column");
        &self.table.rows[self.index].cells[index]
    }
}

impl<'t, 'tb> AsRef<Row<'tb>> for RowRef<'t, 'tb> {
    fn as_ref(&self) -> &'t Row<'tb> {
        &self.table.rows[self.index]
    }
}
