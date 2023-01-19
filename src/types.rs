use enum_kinds::EnumKind;
use num_enum::TryFromPrimitive;
use std::{fmt::Display, ops::Index};

use crate::hash::PreHashedMap;
// doc imports
#[allow(unused_imports)]
use crate::io::BdatVersion;

/// A Bdat table
///
/// ## Accessing cells
/// The [`RawTable::row`] function provides an easy interface to access cells.  
/// For example, to access the cell at row 1 and column "Param1", you can use `table.row(1)["Param1".into()]`.
///
/// ## Example
///
/// ```
/// use bdat::{RawTable, TableBuilder, Cell, ColumnDef, Row, Value, ValueType, Label};
///
/// let table: RawTable = TableBuilder::new()
///     .add_column(ColumnDef::new(ValueType::UnsignedInt, Label::Hash(0xCAFEBABE)))
///     .add_row(Row::new(1, vec![Cell::Single(Value::UnsignedInt(10))]))
///     .build();
///
/// assert_eq!(table.row_count(), 1);
/// assert_eq!(
///     *table.row(1)[Label::Hash(0xCAFEBABE)].as_single().unwrap(),
///     Value::UnsignedInt(10)
/// );
/// ```
#[derive(Debug, Clone, PartialEq, Default)]
pub struct RawTable {
    pub(crate) name: Option<Label>,
    pub(crate) base_id: usize,
    pub(crate) columns: Vec<ColumnDef>,
    pub(crate) rows: Vec<Row>,
    #[cfg(feature = "hash-table")]
    row_hash_table: PreHashedMap<u32, usize>,
}

/// A builder interface for [`RawTable`].
pub struct TableBuilder(RawTable);

/// A column definition from a Bdat table
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ColumnDef {
    pub ty: ValueType,
    pub label: Label,
    pub offset: usize,
}

/// A row from a Bdat table
#[derive(Debug, Clone, PartialEq)]
pub struct Row {
    pub(crate) id: usize,
    pub(crate) cells: Vec<Cell>,
}

/// A cell from a Bdat row
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize), serde(untagged))]
pub enum Cell {
    /// The cell only contains a single [`Value`]. This is the only supported type
    /// in [`BdatVersion::Modern`] BDATs.
    Single(Value),
    /// The cell contains a list of [`Value`]s
    List(Vec<Value>),
    /// The cell acts as a bit flag
    Flag(bool),
}

/// A value in a Bdat cell
#[derive(EnumKind, Debug, Clone, PartialEq)]
#[enum_kind(
    ValueType,
    derive(TryFromPrimitive),
    repr(u8),
    cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize)),
    cfg_attr(feature = "serde", serde(into = "u8", try_from = "u8"))
)]
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
    /// A hash referencing a row in the same or some other table
    HashRef(u32),
    Percent(u8),
    /// [`BdatVersion::Modern`] unknown type (0xb)  
    /// It points to a (generally empty) string in the string table,
    /// mostly used for `DebugName` fields.
    Unknown1(String),
    /// [`BdatVersion::Modern`] unknown type (0xc)
    Unknown2(u8),
    /// [`BdatVersion::Modern`] unknown type (0xd)
    /// It seems to be some sort of translation index, mostly used for
    /// `Name` and `Caption` fields.
    Unknown3(u16),
}

/// A name for a BDAT element (table, column, ID, etc.)
#[derive(PartialEq, Eq, Debug, Clone, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Label {
    /// 32-bit hash, notably used in [`BdatVersion::Modern`] BDATs.
    Hash(u32),
    /// Plain-text string, used in older BDAT formats.
    String(String),
    /// Equivalent to [`Label::String`], but it is made explicit that the label
    /// was originally hashed.
    Unhashed(String),
}

pub struct RowRef<'t> {
    index: usize,
    id: usize,
    table: &'t RawTable,
}

impl Label {
    /// Extracts a [`Label`] from a [`String`].
    ///
    /// The format is as follows:  
    /// * `<01ABCDEF>` (8 hex digits) => `Label::Hash(0x01abcdef)`
    /// * s => `Label::String(s)`
    ///
    /// If `force_hash` is `true`, the label will be re-hashed
    /// if it is either [`Label::String`] or [`Label::Unhashed`].
    pub fn parse(text: String, force_hash: bool) -> Self {
        if text.len() == 10 && text.as_bytes()[0] == b'<' {
            if let Ok(n) = u32::from_str_radix(&text[1..=8], 16) {
                return Label::Hash(n);
            }
        }
        if force_hash {
            Label::Hash(crate::hash::murmur3_str(&text))
        } else {
            Label::String(text)
        }
    }

    /// If needed, turns the label into a hashed label.
    pub fn into_hash(self) -> Self {
        match self {
            l @ Self::Hash(_) => l,
            Self::String(s) | Self::Unhashed(s) => Self::Hash(crate::hash::murmur3_str(&s)),
        }
    }
}

impl RawTable {
    pub fn new(name: Option<Label>, columns: Vec<ColumnDef>, rows: Vec<Row>) -> Self {
        Self {
            name,
            columns,
            base_id: rows.iter().map(|r| r.id).min().unwrap_or_default(),
            rows,
            #[cfg(feature = "hash-table")]
            row_hash_table: Default::default(),
        }
    }

    /// Returns the table's name, or [`None`] if the table has no
    /// name associated to it.
    pub fn name(&self) -> Option<&Label> {
        self.name.as_ref()
    }

    /// Updates the table's name.
    pub fn set_name(&mut self, name: Option<Label>) {
        self.name = name;
    }

    /// Gets the minimum row ID in the table.
    pub fn base_id(&self) -> usize {
        self.base_id
    }

    /// Gets a row by its ID
    ///
    /// Note: the ID is the row's numerical ID, which could be different
    /// from the index of the row in the table's row list. That is because
    /// BDAT tables can have arbitrary start IDs.
    ///
    /// # Panics
    /// If there is no row for the given ID
    pub fn row(&self, id: usize) -> RowRef<'_> {
        self.get_row(id).expect("no such row")
    }

    /// Attempts to get a row by its ID.  
    /// If there is no row for the given ID, this returns [`None`].
    ///
    /// Note: the ID is the row's numerical ID, which could be different
    /// from the index of the row in the table's row list. That is because
    /// BDAT tables can have arbitrary start IDs.
    pub fn get_row(&self, id: usize) -> Option<RowRef<'_>> {
        let index = id - self.base_id;
        self.rows.get(index).map(|_| RowRef {
            index,
            id,
            table: self,
        })
    }

    /// Gets an iterator that visits this table's rows
    pub fn rows(&self) -> impl Iterator<Item = &Row> {
        self.rows.iter()
    }

    /// Gets an iterator over mutable references to this table's
    /// rows.
    pub fn rows_mut(&mut self) -> impl Iterator<Item = &mut Row> {
        self.rows.iter_mut()
    }

    /// Gets an owning iterator over this table's rows
    pub fn into_rows(self) -> impl Iterator<Item = Row> {
        self.rows.into_iter()
    }

    /// Gets an iterator that visits this table's column definitions
    pub fn columns(&self) -> impl Iterator<Item = &ColumnDef> {
        self.columns.iter()
    }

    /// Gets an iterator over mutable references to this table's
    /// column definitions.
    pub fn columns_mut(&mut self) -> impl Iterator<Item = &mut ColumnDef> {
        self.columns.iter_mut()
    }

    /// Gets an owning iterator over this table's column definitions
    pub fn into_columns(self) -> impl Iterator<Item = ColumnDef> {
        self.columns.into_iter()
    }

    /// Gets the number of rows in the table
    pub fn row_count(&self) -> usize {
        self.rows.len()
    }

    /// Gets the number of columns in the table
    pub fn column_count(&self) -> usize {
        self.columns.len()
    }

    /// Attempts to get a row by its hashed 32-bit ID.
    /// If there is no row for the given ID, this returns [`None`].
    ///
    /// This requires the `hash-table` feature flag, which is enabled
    /// by default.
    #[cfg(feature = "hash-table")]
    pub fn get_row_by_hash(&self, hash_id: u32) -> Option<RowRef<'_>> {
        self.row_hash_table
            .get(&hash_id)
            .and_then(|&id| self.get_row(id))
    }
}

impl TableBuilder {
    pub fn new() -> Self {
        Self(RawTable::default())
    }

    pub fn set_name(&mut self, name: impl Into<Option<Label>>) -> &mut Self {
        self.0.set_name(name.into());
        self
    }

    pub fn add_column(&mut self, column: ColumnDef) -> &mut Self {
        self.0.columns.push(column);
        self
    }

    pub fn add_row(&mut self, row: Row) -> &mut Self {
        if self.0.base_id == 0 || self.0.base_id > row.id {
            self.0.base_id = row.id;
        }
        #[cfg(feature = "hash-table")]
        {
            if let Some(id) = row.id_hash() {
                self.0.row_hash_table.insert(id, row.id);
            }
        }
        self.0.rows.push(row);
        self
    }

    pub fn set_rows(&mut self, rows: Vec<Row>) -> &mut Self {
        #[cfg(feature = "hash-table")]
        {
            for row in &rows {
                if let Some(id) = row.id_hash() {
                    self.0.row_hash_table.insert(id, row.id);
                }
            }
        }
        self.0.base_id = rows.iter().map(|r| r.id).min().unwrap_or_default();
        self.0.rows = rows;
        self
    }

    pub fn set_columns(&mut self, columns: Vec<ColumnDef>) -> &mut Self {
        self.0.columns = columns;
        self
    }

    pub fn build(&mut self) -> RawTable {
        std::mem::take(&mut self.0)
    }
}

impl Row {
    /// Creates a new [`Row`].
    pub fn new(id: usize, cells: Vec<Cell>) -> Self {
        Self { id, cells }
    }

    /// Gets the row's ID
    pub fn id(&self) -> usize {
        self.id
    }

    /// Gets an owning iterator over this row's cells
    pub fn into_cells(self) -> impl Iterator<Item = Cell> {
        self.cells.into_iter()
    }

    /// Gets an iterator over this row's cells
    pub fn cells(&self) -> impl Iterator<Item = &Cell> {
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

impl ColumnDef {
    /// Creates a new [`ColumnDef`].
    pub fn new(ty: ValueType, label: Label) -> Self {
        Self {
            ty,
            label,
            offset: 0,
        }
    }
}

impl Cell {
    /// Gets the cell's value, if it is a [`Cell::Single`].
    ///
    /// ## Panics
    /// If the cell is not a [`Cell::Single`].
    pub fn unwrap_single(self) -> Value {
        self.into_single().expect("Cell::Single")
    }

    /// Gets a reference to the cell's value, if it
    /// is a [`Cell::Single`], and returns [`None`]
    /// if it is not.
    pub fn as_single(&self) -> Option<&Value> {
        match self {
            Self::Single(v) => Some(v),
            _ => None,
        }
    }

    /// Gets the cell's value, if it is a [`Cell::Single`], and
    /// returns [`None`] if it is not.
    pub fn into_single(self) -> Option<Value> {
        match self {
            Self::Single(v) => Some(v),
            _ => None,
        }
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

impl<'t> RowRef<'t> {
    /// Returns the row's original ID
    pub fn id(&self) -> usize {
        self.id
    }

    /// Returns a reference to the cell at the given column.
    pub fn get(&self, column: impl AsRef<Label>) -> Option<&'t Cell> {
        let label = column.as_ref();
        let index = self
            .table
            .columns
            .iter()
            .position(|col| col.label == *label)?;
        self.table.rows[self.index].cells.get(index)
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
            Self::Hash(hash) => {
                if f.sign_plus() {
                    write!(f, "{:08X}", hash)
                } else {
                    write!(f, "<{:08X}>", hash)
                }
            }
            Self::String(s) | Self::Unhashed(s) => write!(f, "{}", s),
        }
    }
}

impl Default for TableBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl From<ValueType> for u8 {
    fn from(t: ValueType) -> Self {
        t as u8
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
            Self::Unknown => Ok(()),
            Self::HashRef(h) => Label::Hash(*h).fmt(f),
            Self::Percent(v) => write!(f, "{}%", v),
            v => {
                default_display!(f, v, SignedByte SignedShort SignedInt UnsignedByte UnsignedShort UnsignedInt Unknown1 Unknown2 Unknown3 String Float)
            }
        }
    }
}

impl Value {
    /// Returns the integer representation of this value.
    /// For signed values, this is the unsigned representation.
    ///
    /// # Panics
    /// If the value is not stored as an integer.
    /// Do not use this for floats, use [`Value::into_float`] instead.
    pub fn into_integer(self) -> u32 {
        match self {
            Self::SignedByte(b) => b as u32,
            Self::Percent(b) | Self::UnsignedByte(b) | Self::Unknown2(b) => b as u32,
            Self::SignedShort(s) => s as u32,
            Self::UnsignedShort(s) | Self::Unknown3(s) => s as u32,
            Self::SignedInt(i) => i as u32,
            Self::UnsignedInt(i) | Self::HashRef(i) => i,
            _ => panic!("value is not an integer"),
        }
    }

    /// Returns the floating point representation of this value.
    ///
    /// # Panics
    /// If the value is not stored as a float.
    pub fn into_float(self) -> f32 {
        match self {
            Self::Float(f) => f,
            _ => panic!("value is not a float"),
        }
    }

    /// Returns the underlying string value.
    /// This does **not** format other values, use the Display trait for that.
    ///
    /// # Panics
    /// If the value is not stored as a string.
    pub fn into_string(self) -> String {
        match self {
            Self::String(s) | Self::Unknown1(s) => s,
            _ => panic!("value is not a string"),
        }
    }
}

#[cfg(test)]
mod tests {
    #[cfg(feature = "hash-table")]
    #[test]
    fn test_hash_table() {
        use crate::{Cell, ColumnDef, Label, Row, TableBuilder, Value, ValueType};

        let table = TableBuilder::new()
            .add_column(ColumnDef::new(ValueType::HashRef, 0.into()))
            .add_column(ColumnDef::new(ValueType::UnsignedInt, 1.into()))
            .add_row(Row::new(
                1,
                vec![
                    Cell::Single(Value::HashRef(0xabcdef01)),
                    Cell::Single(Value::UnsignedInt(256)),
                ],
            ))
            .add_row(Row::new(
                2,
                vec![
                    Cell::Single(Value::HashRef(0xdeadbeef)),
                    Cell::Single(Value::UnsignedInt(100)),
                ],
            ))
            .build();
        assert_eq!(1, table.get_row_by_hash(0xabcdef01).unwrap().id());
        assert_eq!(2, table.get_row_by_hash(0xdeadbeef).unwrap().id());
        assert_eq!(
            256,
            table.get_row_by_hash(0xabcdef01).unwrap()[Label::Hash(1)]
                .as_single()
                .unwrap()
                .clone()
                .into_integer()
        );
        assert_eq!(
            100,
            table.get_row_by_hash(0xdeadbeef).unwrap()[Label::Hash(1)]
                .as_single()
                .unwrap()
                .clone()
                .into_integer()
        );
    }
}
