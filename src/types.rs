use std::ops::Index;

/// A Bdat table
///
/// ## Accessing cells
/// The [`RowRef`] struct provides an easy interface to access cells.  
/// For example, to access the cell at row 1 and column "Param1", you can use `table.row(1)["Param1"]`.
pub struct Table {
    name: Label,
    columns: Vec<String>,
    rows: Vec<Cell>,
}

/// A cell in a Bdat table
pub enum Cell {
    Single(Value),
    List(Vec<Value>),
    Flag(bool),
}

/// A value in a Bdat cell
pub enum Value {
    UnsignedByte(u8),
    UnsignedShort(u16),
    UnsignedInt(u32),
    SignedByte(i8),
    SignedShort(i16),
    SignedInt(i32),
    String(String),
    Float(f32),
    HashRef(u32),
    Percent(f32),
    Unknown1(u32),
    Unknown2(u8),
    Unknown3(u16),
}

pub enum Label {
    Hash(u32),
    String(String),
}

struct RowRef<'t> {
    index: usize,
    table: &'t Table,
}

impl Table {
    /// Gets a row by its ID
    ///
    /// # Panics
    /// If there is no row for the given ID
    pub fn row(&self, id: usize) -> RowRef<'_> {
        self.get_row(id).expect("no such row")
    }

    /// Attempts to get a row by its ID.  
    /// If there is no row for the given ID, this returns [`None`].
    pub fn get_row(&self, id: usize) -> Option<RowRef<'_>> {
        self.rows.get(id).map(|_| RowRef {
            index: id,
            table: self,
        })
    }
}

impl<'t, S> Index<S> for RowRef<'t>
where
    S: AsRef<str>,
{
    type Output = Cell;

    fn index(&self, index: S) -> &Self::Output {
        let index = index.as_ref();
        let index = self
            .table
            .columns
            .iter()
            .position(|col| col == index)
            .expect("no such column");
        &self.table.rows[index]
    }
}
