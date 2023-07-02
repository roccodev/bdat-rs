use crate::Table;
use crate::{Cell, Label, Value};
use std::ops::Index;

/// A row from a Bdat table
#[derive(Debug, Clone, PartialEq)]
pub struct Row<'b> {
    pub id: usize,
    pub cells: Vec<Cell<'b>>,
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
///     // Use the index syntax to access cells
///     row["Param1"].as_single().unwrap().to_integer()
/// }
///
/// fn param_2_if_present(row: RowRef) -> Option<u32> {
///     // Or use .get() for columns that might be absent
///     row.get("Param2")
///         .and_then(|cell| cell.as_single())
///         .map(|value| value.to_integer())
/// }
/// ```
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

impl<'a, 't: 'a, 'tb> RowRef<'t, 'tb> {
    pub(crate) fn new(table: &'t Table<'tb>, index: usize, id: usize) -> Self {
        Self { index, id, table }
    }

    /// Returns the row's original ID
    pub fn id(&self) -> usize {
        self.id
    }

    /// Returns a reference to the cell at the given column.
    pub fn get<K>(&self, column: K) -> Option<&'t Cell<'tb>>
    where
        K: TryFrom<&'a Label> + PartialEq,
    {
        let index = self
            .table
            .columns
            .iter()
            .position(|col| matches!(K::try_from(col.label()), Ok(v) if v == column))?;
        self.table.rows[self.index].cells.get(index)
    }

    /// Returns the table this row belongs to.
    pub fn table(&self) -> &'t Table<'tb> {
        self.table
    }
}

impl<'t, 'tb> Index<Label> for RowRef<'t, 'tb> {
    type Output = Cell<'tb>;

    fn index(&self, index: Label) -> &Self::Output {
        &self[&index]
    }
}

// Implementation for e.g. row["string slice"]
impl<'a, 't: 'a, 'tb, S> Index<S> for RowRef<'t, 'tb>
where
    S: TryFrom<&'a Label> + PartialEq,
{
    type Output = Cell<'tb>;

    fn index(&self, index: S) -> &Self::Output {
        self.get(index).expect("no such column")
    }
}

impl<'t, 'tb> AsRef<Row<'tb>> for RowRef<'t, 'tb> {
    fn as_ref(&self) -> &'t Row<'tb> {
        &self.table.rows[self.index]
    }
}
