use std::{marker::PhantomData, ops::Index};

/// A deserialized Bdat table
pub struct Table<R> {
    rows: Vec<R>,
    columns: usize,
}

/// A memory-mapped Bdat table
pub struct MappedTable<'b, I, R> {
    buffer: &'b I,
    _ty: PhantomData<R>,
}

/// A Bdat table
///
/// ## Accessing cells
/// The [`RowRef`] struct provides an easy interface to access cells.  
/// For example, to access the cell at row 1 and column "Param1", you can use `table.row(1)["Param1".into()]`.
pub struct RawTable {
    pub(crate) name: Option<Label>,
    pub(crate) columns: Vec<ColumnDef>,
    pub(crate) rows: Vec<Vec<Cell>>,
}

/// A column definition from a Bdat table
pub struct ColumnDef {
    pub(crate) ty: u8,
    pub(crate) label: Label,
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

#[derive(PartialEq, Eq)]
pub enum Label {
    Hash(u32),
    String(String),
}

pub struct RowRef<'t> {
    index: usize,
    table: &'t RawTable,
}

impl<R> Table<R> {
    pub fn len(&self) -> usize {
        self.rows.len()
    }

    pub fn columns(&self) -> usize {
        self.columns
    }
}

impl RawTable {
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
    S: Into<Label>,
{
    type Output = Cell;

    fn index(&self, index: S) -> &Self::Output {
        let index = index.into();
        let index = self
            .table
            .columns
            .iter()
            .position(|col| col.label == index)
            .expect("no such column");
        &self.table.rows[self.index][index]
    }
}

impl From<String> for Label {
    fn from(s: String) -> Self {
        Self::String(s)
    }
}

impl From<u32> for Label {
    fn from(hash: u32) -> Self {
        Self::Hash(hash)
    }
}
