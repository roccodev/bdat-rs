use std::{fmt::Display, marker::PhantomData, ops::Index};

use enum_kinds::EnumKind;
use num_enum::TryFromPrimitive;

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
    pub name: Option<Label>,
    pub columns: Vec<ColumnDef>,
    pub rows: Vec<Row>,
}

/// A column definition from a Bdat table
#[derive(Debug)]
pub struct ColumnDef {
    pub ty: ValueType,
    pub label: Label,
    pub offset: usize,
}

/// A row from a Bdat table
#[derive(Debug)]
pub struct Row {
    pub id: usize,
    pub cells: Vec<Cell>,
}

/// A cell from a Bdat row
#[derive(Debug)]
pub enum Cell {
    Single(Value),
    List(Vec<Value>),
    Flag(bool),
}

/// A value in a Bdat cell
#[derive(EnumKind, Debug)]
#[enum_kind(ValueType, derive(TryFromPrimitive), repr(u8))]
pub enum Value {
    Unknown,
    UnsignedByte(u8),
    UnsignedShort(u16),
    UnsignedInt(u32),
    SignedByte(i8),
    SignedShort(i16),
    SignedInt(i32),
    String(String),
    Float(f32),
    HashRef(u32),
    Percent(u8),
    Unknown1(u32),
    Unknown2(u8),
    Unknown3(u16),
}

#[derive(PartialEq, Eq, Debug)]
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

impl ValueType {
    pub fn data_len(&self) -> usize {
        use ValueType::*;
        match self {
            Unknown => 0,
            UnsignedByte | SignedByte | Percent | Unknown2 => 1,
            UnsignedShort | SignedShort | Unknown3 => 2,
            UnsignedInt | SignedInt | String | Float | HashRef | Unknown1 => 4,
        }
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
        &self.table.rows[self.index].cells[index]
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

impl Display for Label {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Hash(hash) => write!(f, "<{:08X}>", hash), // TODO
            Self::String(s) => write!(f, "{}", s),
        }
    }
}

macro_rules! default_display {
    ($fmt:expr, $val:expr, $($variants:tt ) *) => {
        match $val {
            $(
                Value::$variants(a) => a.fmt($fmt),
            )*
            v => panic!("Unsupported Display {:?}", v)
        }
    };
}

impl Display for Value {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unknown => return Ok(()),
            Self::HashRef(h) => Label::Hash(*h).fmt(f),
            Self::Percent(v) => write!(f, "{}%", v),
            v => {
                default_display!(f, v, SignedByte SignedShort SignedInt UnsignedByte UnsignedShort UnsignedInt Unknown1 Unknown2 Unknown3 String Float)
            }
        }
    }
}
