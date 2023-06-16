use enum_kinds::EnumKind;
use num_enum::TryFromPrimitive;
use std::borrow::Cow;
use std::{borrow::Borrow, cmp::Ordering, fmt::Display, ops::Index};

#[cfg(feature = "hash-table")]
use crate::hash::PreHashedMap;
// doc imports
#[allow(unused_imports)]
use crate::io::BdatVersion;
use crate::legacy::float::BdatReal;

/// A Bdat table. Depending on how they were read, BDAT tables can either own their data source
/// or borrow from it.
///
/// ## Accessing cells
/// The [`Table::row`] function provides an easy interface to access cells.
/// For example, to access the cell at row 1 and column "Param1", you can use `table.row(1)["Param1".into()]`.
///
/// ## Example
///
/// ```
/// use bdat::{Table, TableBuilder, Cell, ColumnDef, Row, Value, ValueType, Label};
///
/// let table: Table = TableBuilder::new()
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
pub struct Table<'b> {
    pub(crate) name: Option<Label>,
    pub(crate) base_id: usize,
    pub(crate) columns: Vec<ColumnDef>,
    pub(crate) rows: Vec<Row<'b>>,
    #[cfg(feature = "hash-table")]
    row_hash_table: PreHashedMap<u32, usize>,
}

/// A builder interface for [`Table`].
pub struct TableBuilder<'b>(Table<'b>);

/// A column definition from a Bdat table
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ColumnDef {
    pub(crate) value_type: ValueType,
    pub(crate) label: Label,
    pub(crate) offset: usize,
    pub(crate) count: usize,
    pub(crate) flags: Vec<FlagDef>,
}

/// A row from a Bdat table
#[derive(Debug, Clone, PartialEq)]
pub struct Row<'b> {
    pub(crate) id: usize,
    pub(crate) cells: Vec<Cell<'b>>,
}

/// A sub-definition for flag data that is associated to a column
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FlagDef {
    /// The flag's identifier
    pub(crate) label: Label,
    /// The bits this flag is setting on the parent
    pub(crate) mask: u32,
    /// The index in the parent cell's flag list
    pub(crate) flag_index: usize,
}

/// A cell from a Bdat row
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize), serde(untagged))]
pub enum Cell<'b> {
    /// The cell only contains a single [`Value`]. This is the only supported type
    /// in [`BdatVersion::Modern`] BDATs.
    Single(Value<'b>),
    /// The cell contains a list of [`Value`]s
    List(Vec<Value<'b>>),
    /// The cell acts as a list of integers, derived by masking bits from the
    /// parent value.
    Flags(Vec<u32>),
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
pub enum Value<'b> {
    Unknown,
    UnsignedByte(u8),
    UnsignedShort(u16),
    UnsignedInt(u32),
    SignedByte(i8),
    SignedShort(i16),
    SignedInt(i32),
    String(Cow<'b, str>),
    Float(BdatReal),
    /// A hash referencing a row in the same or some other table
    HashRef(u32),
    Percent(u8),
    /// It points to a (generally empty) string in the string table,
    /// mostly used for `DebugName` fields.
    DebugString(Cow<'b, str>),
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

pub struct RowRef<'t, 'tb> {
    index: usize,
    id: usize,
    table: &'t Table<'tb>,
}

pub struct RowIter<'t, 'tb> {
    table: &'t Table<'tb>,
    row_id: usize,
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

    /// Comparison function for the underlying values.
    ///
    /// Unlike a typical [`Ord`] implementation for enums, this only takes values into consideration
    /// (though hashed labels are still considered separately), meaning the following holds:
    ///
    /// ```rs
    /// use bdat::Label;
    /// use std::cmp::Ordering;
    ///
    /// assert_eq!(Label::Hash(0x0).cmp_value(&Label::Hash(0x0)), Ordering::Equal);
    /// assert_eq!(Label::String("Test".to_string()).cmp_value(&Label::String("Test".to_string())), Ordering::Equal);
    /// // and...
    /// assert_eq!(Label::String("Test".to_string()).cmp_value(&Label::Unhashed("Test".to_string())), Ordering::Equal);
    /// // ...but not
    /// assert_ne!(Label::String(String::new()).cmp_value(&Label::Hash(0x0)), Ordering::Equal);
    /// ```
    pub fn cmp_value(&self, other: &Self) -> Ordering {
        match (self, other) {
            (Self::Hash(slf), Self::Hash(oth)) => slf.cmp(oth),
            (_, Self::Hash(_)) => Ordering::Less, // hashed IDs always come last
            (Self::Hash(_), _) => Ordering::Greater,
            (a, b) => a.as_str().cmp(b.as_str()),
        }
    }

    fn as_str(&self) -> &str {
        match self {
            Self::String(s) | Self::Unhashed(s) => s.as_str(),
            _ => panic!("label is not a string"),
        }
    }
}

impl<'b> Table<'b> {
    pub fn new(name: Option<Label>, columns: Vec<ColumnDef>, rows: Vec<Row<'b>>) -> Self {
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
    pub fn row(&self, id: usize) -> RowRef<'_, 'b> {
        self.get_row(id).expect("no such row")
    }

    /// Attempts to get a row by its ID.  
    /// If there is no row for the given ID, this returns [`None`].
    ///
    /// Note: the ID is the row's numerical ID, which could be different
    /// from the index of the row in the table's row list. That is because
    /// BDAT tables can have arbitrary start IDs.
    pub fn get_row(&self, id: usize) -> Option<RowRef<'_, 'b>> {
        let index = id.checked_sub(self.base_id)?;
        self.rows.get(index).map(|_| RowRef {
            index,
            id,
            table: self,
        })
    }

    /// Gets an iterator that visits this table's rows
    pub fn rows(&self) -> impl Iterator<Item = &Row<'b>> {
        self.rows.iter()
    }

    /// Gets an iterator over mutable references to this table's
    /// rows.
    pub fn rows_mut(&mut self) -> impl Iterator<Item = &mut Row<'b>> {
        self.rows.iter_mut()
    }

    /// Gets an owning iterator over this table's rows
    pub fn into_rows(self) -> impl Iterator<Item = Row<'b>> {
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
    pub fn get_row_by_hash(&self, hash_id: u32) -> Option<RowRef<'_, 'b>> {
        self.row_hash_table
            .get(&hash_id)
            .and_then(|&id| self.get_row(id))
    }

    /// Returns an ergonomic iterator view over the table's rows and columns.
    pub fn iter(&self) -> RowIter {
        self.into_iter()
    }
}

impl<'b> TableBuilder<'b> {
    pub fn new() -> Self {
        Self(Table::default())
    }

    pub fn set_name(&mut self, name: impl Into<Option<Label>>) -> &mut Self {
        self.0.set_name(name.into());
        self
    }

    pub fn add_column(&mut self, column: ColumnDef) -> &mut Self {
        self.0.columns.push(column);
        self
    }

    pub fn add_row(&mut self, row: Row<'b>) -> &mut Self {
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

    pub fn set_rows(&mut self, rows: Vec<Row<'b>>) -> &mut Self {
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

    pub fn build(&mut self) -> Table<'b> {
        std::mem::take(&mut self.0)
    }
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

impl ColumnDef {
    /// Creates a new [`ColumnDef`].
    pub fn new(ty: ValueType, label: Label) -> Self {
        Self {
            value_type: ty,
            label,
            offset: 0,
            flags: Vec::new(),
            count: 1,
        }
    }

    /// Returns this column's type.
    pub fn value_type(&self) -> ValueType {
        self.value_type
    }

    /// Returns this column's name.
    pub fn label(&self) -> &Label {
        &self.label
    }

    /// Returns a mutable reference to this column's name.
    pub fn label_mut(&mut self) -> &mut Label {
        &mut self.label
    }

    /// Returns this column's data offset.
    pub fn offset(&self) -> usize {
        self.offset
    }
}

impl<'b> Cell<'b> {
    /// Gets the cell's value, if it is a [`Cell::Single`].
    ///
    /// ## Panics
    /// If the cell is not a [`Cell::Single`].
    pub fn unwrap_single(self) -> Value<'b> {
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
    pub fn into_single(self) -> Option<Value<'b>> {
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
            UnsignedInt | SignedInt | String | Float | HashRef | DebugString => 4,
        }
    }
}

impl<'t, 'tb> RowRef<'t, 'tb> {
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

impl<'t, 'tb> Iterator for RowIter<'t, 'tb> {
    type Item = RowRef<'t, 'tb>;

    fn next(&mut self) -> Option<Self::Item> {
        let item = self.table.get_row(self.row_id)?;
        self.row_id += 1;
        Some(item)
    }
}

impl<'t, 'tb> IntoIterator for &'t Table<'tb> {
    type Item = RowRef<'t, 'tb>;
    type IntoIter = RowIter<'t, 'tb>;

    fn into_iter(self) -> Self::IntoIter {
        RowIter {
            table: self,
            row_id: self.base_id(),
        }
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

impl<'b> Default for TableBuilder<'b> {
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

impl<'b> Display for Value<'b> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unknown => Ok(()),
            Self::HashRef(h) => Label::Hash(*h).fmt(f),
            Self::Percent(v) => write!(f, "{}%", v),
            v => {
                default_display!(f, v, SignedByte SignedShort SignedInt UnsignedByte UnsignedShort UnsignedInt DebugString Unknown2 Unknown3 String Float)
            }
        }
    }
}

impl<'b> Display for Cell<'b> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Single(val) => val.fmt(f),
            Cell::List(list) => {
                write!(f, "[")?;
                for (i, value) in list.iter().enumerate() {
                    if i != 0 {
                        write!(f, ", ")?;
                    }
                    value.fmt(f)?;
                }
                write!(f, "]")
            }
            Cell::Flags(b) => todo!(), /*b.fmt(f) */
        }
    }
}

impl<'b> Value<'b> {
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
            Self::Float(f) => f.into(),
            _ => panic!("value is not a float"),
        }
    }

    /// Returns the underlying string value.
    /// This does **not** format other values, use the Display trait for that.
    ///
    /// **Note**: this will potentially copy the string, if the table is borrowing its source.
    /// To avoid copies, use [`Value::as_str`].
    ///
    /// # Panics
    /// If the value is not stored as a string.
    pub fn into_string(self) -> String {
        match self {
            Self::String(s) | Self::DebugString(s) => s.to_string(),
            _ => panic!("value is not a string"),
        }
    }

    /// Returns a reference to the underlying string value.
    ///
    /// # Panics
    /// If the value is not stored as a string.
    pub fn as_str(&self) -> &str {
        match self {
            Self::String(s) | Self::DebugString(s) => s.as_ref(),
            _ => panic!("value is not a string"),
        }
    }
}

impl<'t, 'tb> AsRef<Row<'tb>> for RowRef<'t, 'tb> {
    fn as_ref(&self) -> &'t Row<'tb> {
        &self.table.rows[self.index]
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
