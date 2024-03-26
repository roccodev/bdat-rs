//! Adapters for legacy<->modern BDAT compatibility.

use std::borrow::Borrow;
use std::ops::Deref;

use super::builder::{CompatBuilderRow, CompatColumnBuilder};
use super::legacy::LegacyRow;
use super::modern::ModernRow;
use super::private::ColumnSerialize;
use super::util::CompatIter;
use super::Table;
use crate::{
    BdatResult, Cell, CellAccessor, ColumnMap, Label, LabelMap, LegacyColumn, LegacyFlag,
    LegacyTable, ModernColumn, ModernTable, NameMap, RowId, RowRef, Utf, ValueType,
};

/// A BDAT table. Depending on how they were read, BDAT tables can either own their data source
/// or borrow from it.
///
/// ## Accessing cells
/// The [`Table::row`] function provides an easy interface to access cells.
///
/// See also: [`RowRef`]
///
/// ## Specialized views
/// If you know what type of BDAT tables you're dealing with (legacy or modern), you can use
/// [`as_modern`] and [`as_legacy`] to get specialized table views.
///
/// These views return more ergonomic row accessors that let you quickly extract values, instead
/// of having to handle cases that are not supported by the known version.
///
/// See also: [`ModernTable`], [`LegacyTable`]
///
/// ## Adding/deleting rows
/// The table's mutable iterator does not allow structural modifications to the table. To add or
/// delete rows, re-build the table. (`CompatTableBuilder::from(table)`)
///
/// ## Examples
///
/// ```
/// use bdat::{CompatTable, CompatTableBuilder, Cell, Column, Value, ValueType, Label, BdatVersion};
///
/// let table: CompatTable = CompatTableBuilder::with_name(Label::Hash(0xDEADBEEF))
///     .set_base_id(1) // default, if you want 0 it must be set manually
///     .add_column(Column::new(ValueType::UnsignedInt, Label::Hash(0xCAFEBABE)))
///     .add_row(vec![Cell::Single(Value::UnsignedInt(10))].into())
///     .build(BdatVersion::Modern);
///
/// assert_eq!(table.row_count(), 1);
/// assert_eq!(
///     *table.row(1).get(Label::Hash(0xCAFEBABE)).as_single().unwrap(),
///     Value::UnsignedInt(10)
/// );
/// ```
///
/// [`as_legacy`]: Table::as_legacy
/// [`as_modern`]: Table::as_modern
#[derive(Debug, Clone, PartialEq)]
pub struct CompatTable<'b> {
    inner: CompatInner<'b>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum CompatInner<'b> {
    Modern(ModernTable<'b>),
    Legacy(LegacyTable<'b>),
}

pub enum CompatRow<'buf> {
    Modern(ModernRow<'buf>),
    Legacy(LegacyRow<'buf>),
}

#[derive(Clone, Copy)]
pub enum CompatRef<'t, 'buf> {
    Modern(&'t ModernRow<'buf>),
    Legacy(&'t LegacyRow<'buf>),
}

#[derive(Clone)]
pub enum CompatColumn<'buf> {
    Modern(ModernColumn<'buf>),
    Legacy(LegacyColumn<'buf>),
}

#[derive(Clone, Copy)]
pub enum CompatColumnRef<'t, 'buf> {
    Modern(&'t ModernColumn<'buf>),
    Legacy(&'t LegacyColumn<'buf>),
}

#[derive(Clone)]
pub enum CompatColumnMap<'t, 'buf> {
    Modern(&'t ColumnMap<ModernColumn<'buf>>),
    Legacy(&'t ColumnMap<LegacyColumn<'buf>>),
}

pub type CompatRowRef<'t, 'buf> = RowRef<CompatRef<'t, 'buf>, CompatColumnMap<'t, 'buf>>;

macro_rules! versioned {
    ($var:expr, $name:ident) => {
        match $var {
            CompatInner::Modern(m) => &m.$name,
            CompatInner::Legacy(l) => &l.$name,
        }
    };
    ($var:expr, $name:ident($($par:expr ) *)) => {
        match $var {
            CompatInner::Modern(m) => m . $name ( $($par, )* ),
            CompatInner::Legacy(l) => l . $name ( $($par, )* ),
        }
    };
}

impl<'b> CompatTable<'b> {
    pub(crate) fn from_inner(inner: CompatInner<'b>) -> Self {
        Self { inner }
    }

    /// If the table is modern, returns a view of the underlying table.
    ///
    /// ## Panics
    /// Panics if the table is not modern.
    pub fn as_modern(&self) -> &ModernTable<'b> {
        match &self.inner {
            CompatInner::Modern(m) => m,
            _ => panic!("not modern"),
        }
    }

    /// If the table is legacy, returns a view of the underlying table.
    ///
    /// ## Panics
    /// Panics if the table is not legacy.
    pub fn as_legacy(&self) -> &LegacyTable<'b> {
        match &self.inner {
            CompatInner::Legacy(l) => l,
            _ => panic!("not legacy"),
        }
    }

    /// If the table is modern, returns a mutable view of the underlying table.
    ///
    /// ## Panics
    /// Panics if the table is not modern.
    pub fn as_modern_mut(&mut self) -> &mut ModernTable<'b> {
        match &mut self.inner {
            CompatInner::Modern(m) => m,
            _ => panic!("not modern"),
        }
    }

    /// If the table is legacy, returns a mutable view of the underlying table.
    ///
    /// ## Panics
    /// Panics if the table is not legacy.
    pub fn as_legacy_mut(&mut self) -> &mut LegacyTable<'b> {
        match &mut self.inner {
            CompatInner::Legacy(l) => l,
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
            CompatInner::Modern(m) => m,
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
            CompatInner::Legacy(l) => l,
            _ => panic!("not legacy"),
        }
    }

    /// Returns whether the underlying table is modern.
    pub fn is_modern(&self) -> bool {
        matches!(self.inner, CompatInner::Modern(_))
    }

    /// Returns whether the underlying table is legacy.
    pub fn is_legacy(&self) -> bool {
        matches!(self.inner, CompatInner::Legacy(_))
    }

    /// Returns a modern table as close to the underlying table as possible.
    ///
    /// * If the table is modern, this does nothing and returns it.
    /// * If the table is legacy, it tries to convert it to the
    /// modern format, and returns the result.
    pub fn try_into_modern(self) -> BdatResult<ModernTable<'b>> {
        match self.inner {
            CompatInner::Modern(m) => Ok(m),
            CompatInner::Legacy(l) => Ok(l.try_into()?),
        }
    }

    /// Returns a legacy table as close to the underlying table as possible.
    ///
    /// * If the table is legacy, this does nothing and returns it.
    /// * If the table is modern, it tries to convert it to the
    /// legacy format, and returns the result.
    pub fn try_into_legacy(self) -> BdatResult<LegacyTable<'b>> {
        match self.inner {
            CompatInner::Modern(m) => Ok(m.try_into()?),
            CompatInner::Legacy(l) => Ok(l),
        }
    }

    pub fn name(&self) -> Label {
        match &self.inner {
            CompatInner::Modern(m) => m.name().as_ref(),
            CompatInner::Legacy(l) => l.name().into(),
        }
    }

    pub(crate) fn name_cloned(&self) -> Label<'b> {
        match &self.inner {
            CompatInner::Modern(m) => m.name.clone(),
            CompatInner::Legacy(l) => l.name.clone().into(),
        }
    }

    pub fn set_name(&mut self, name: Label<'b>) {
        match &mut self.inner {
            CompatInner::Modern(m) => m.set_name(name),
            CompatInner::Legacy(l) => {
                l.set_name(name.try_into().expect("hashed labels are not supported"))
            }
        }
    }

    /// Gets the minimum row ID in the table.
    pub fn base_id(&self) -> RowId {
        match &self.inner {
            CompatInner::Modern(m) => m.base_id(),
            CompatInner::Legacy(l) => l.base_id() as u32,
        }
    }

    /// Gets a row by its ID.
    ///
    /// Note: the ID is the row's numerical ID, which could be different
    /// from the index of the row in the table's row list. That is because
    /// BDAT tables can have arbitrary start IDs.
    ///
    /// ## Panics
    /// If there is no row for the given ID.
    pub fn row(&self, id: RowId) -> CompatRowRef<'_, 'b> {
        match &self.inner {
            CompatInner::Modern(m) => m
                .row(id)
                .map(CompatRef::Modern, CompatColumnMap::Modern(&m.columns)),
            CompatInner::Legacy(l) => l
                .row(id.try_into().expect("invalid id for legacy row"))
                .map(CompatRef::Legacy, CompatColumnMap::Legacy(&l.columns)),
        }
    }

    /// Attempts to get a row by its ID.  
    /// If there is no row for the given ID, this returns [`None`].
    ///
    /// Note: the ID is the row's numerical ID, which could be different
    /// from the index of the row in the table's row list. That is because
    /// BDAT tables can have arbitrary start IDs.
    pub fn get_row(&self, id: RowId) -> Option<CompatRowRef<'_, 'b>> {
        match &self.inner {
            CompatInner::Modern(m) => m
                .get_row(id)
                .map(|r| r.map(CompatRef::Modern, CompatColumnMap::Modern(&m.columns))),
            CompatInner::Legacy(l) => id
                .try_into()
                .ok()
                .and_then(|id| l.get_row(id))
                .map(|r| r.map(CompatRef::Legacy, CompatColumnMap::Legacy(&l.columns))),
        }
    }

    /// Gets an iterator that visits this table's rows
    pub fn rows(&self) -> impl Iterator<Item = CompatRowRef<'_, 'b>> {
        match &self.inner {
            CompatInner::Modern(m) => CompatIter::Modern(
                m.rows()
                    .map(|r| r.map(CompatRef::Modern, CompatColumnMap::Modern(&m.columns))),
            ),
            CompatInner::Legacy(l) => CompatIter::Legacy(
                l.rows()
                    .map(|r| r.map(CompatRef::Legacy, CompatColumnMap::Legacy(&l.columns))),
            ),
        }
    }

    /// Gets an owning iterator over this table's rows
    pub fn into_rows(self) -> impl Iterator<Item = CompatRow<'b>> {
        match self.inner {
            CompatInner::Modern(m) => CompatIter::Modern(m.into_rows().map(CompatRow::Modern)),
            CompatInner::Legacy(l) => CompatIter::Legacy(l.into_rows().map(CompatRow::Legacy)),
        }
    }

    /// Gets an owning iterator over this table's rows, in pairs of
    /// `(row ID, row)`.
    pub fn into_rows_id(self) -> impl Iterator<Item = (u32, CompatRow<'b>)> {
        match self.inner {
            CompatInner::Modern(m) => {
                CompatIter::Modern(m.into_rows_id().map(|(id, r)| (id, CompatRow::Modern(r))))
            }
            CompatInner::Legacy(l) => CompatIter::Legacy(
                l.into_rows_id()
                    .map(|(id, r)| (id as u32, CompatRow::Legacy(r))),
            ),
        }
    }

    /// Gets an iterator that visits this table's column definitions
    pub fn columns(&self) -> impl Iterator<Item = CompatColumnRef<'_, 'b>> {
        match &self.inner {
            CompatInner::Modern(m) => CompatIter::Modern(m.columns().map(CompatColumnRef::Modern)),
            CompatInner::Legacy(l) => CompatIter::Legacy(l.columns().map(CompatColumnRef::Legacy)),
        }
    }

    /// Gets an owning iterator over this table's column definitions.
    ///
    /// Columns from modern tables will be returned as-is. In the case of legacy
    /// tables, column names are wrapped into the [`Label`] type.
    pub fn into_columns(self) -> impl Iterator<Item = CompatColumn<'b>> {
        match self.inner {
            CompatInner::Modern(m) => {
                CompatIter::Modern(m.into_columns().map(CompatColumn::Modern))
            }
            CompatInner::Legacy(l) => {
                CompatIter::Legacy(l.into_columns().map(CompatColumn::Legacy))
            }
        }
    }

    pub fn row_count(&self) -> usize {
        versioned!(&self.inner, row_count())
    }

    pub fn column_count(&self) -> usize {
        versioned!(&self.inner, column_count())
    }
}

impl<'b> CompatColumn<'b> {
    pub fn as_ref(&self) -> CompatColumnRef<'_, 'b> {
        match self {
            CompatColumn::Modern(m) => CompatColumnRef::Modern(m),
            CompatColumn::Legacy(l) => CompatColumnRef::Legacy(l),
        }
    }
}

impl<'buf> CompatColumn<'buf> {
    pub fn label(&self) -> Label {
        match self {
            Self::Modern(m) => m.label().as_ref(),
            Self::Legacy(l) => l.label().into(),
        }
    }

    pub fn value_type(&self) -> ValueType {
        self.as_ref().value_type()
    }

    pub fn flags(&self) -> &[LegacyFlag<'buf>] {
        match self {
            Self::Modern(_) => &[],
            Self::Legacy(l) => l.flags(),
        }
    }

    pub fn count(&self) -> usize {
        self.as_ref().count()
    }

    pub fn data_size(&self) -> usize {
        self.as_ref().data_size()
    }
}

impl<'t, 'buf> CompatColumnRef<'t, 'buf> {
    pub fn label(&self) -> Label {
        match self {
            Self::Modern(m) => m.label().as_ref(),
            Self::Legacy(l) => l.label().into(),
        }
    }

    pub fn value_type(&self) -> ValueType {
        match self {
            Self::Modern(m) => m.value_type(),
            Self::Legacy(l) => l.value_type(),
        }
    }

    pub fn flags(&self) -> &[LegacyFlag<'buf>] {
        match self {
            Self::Modern(_) => &[],
            Self::Legacy(l) => l.flags(),
        }
    }

    pub fn count(&self) -> usize {
        match self {
            Self::Modern(_) => 1,
            Self::Legacy(l) => l.count(),
        }
    }

    pub fn data_size(&self) -> usize {
        match self {
            Self::Modern(m) => m.data_size(),
            Self::Legacy(l) => l.data_size(),
        }
    }
}

impl<'b> CompatRow<'b> {
    pub fn cells(&self) -> impl Iterator<Item = Cell<'b>> + '_ {
        match self {
            CompatRow::Modern(m) => {
                CompatIter::Modern(m.values.iter().map(|v| Cell::Single(v.clone())))
            }
            CompatRow::Legacy(l) => CompatIter::Legacy(l.cells.iter().cloned()),
        }
    }

    pub fn into_cells(self) -> impl Iterator<Item = Cell<'b>> {
        match self {
            CompatRow::Modern(m) => CompatIter::Modern(m.into_values().map(Cell::Single)),
            CompatRow::Legacy(l) => CompatIter::Legacy(l.into_cells()),
        }
    }
}

impl<'t, 'b> CompatRef<'t, 'b> {
    pub fn cells(&self) -> impl Iterator<Item = Cell<'b>> + '_ {
        match self {
            CompatRef::Modern(m) => {
                CompatIter::Modern(m.values.iter().map(|v| Cell::Single(v.clone())))
            }
            CompatRef::Legacy(l) => CompatIter::Legacy(l.cells.iter().cloned()),
        }
    }
}

impl<'t, 'buf> From<&'t LegacyColumn<'buf>> for CompatColumnRef<'t, 'buf> {
    fn from(value: &'t LegacyColumn<'buf>) -> Self {
        Self::Legacy(value)
    }
}

impl<'t, 'buf> From<&'t ModernColumn<'buf>> for CompatColumnRef<'t, 'buf> {
    fn from(value: &'t ModernColumn<'buf>) -> Self {
        Self::Modern(value)
    }
}

impl<'buf> Table<'buf> for CompatTable<'buf> {
    type Id = RowId;
    type Name = Label<'buf>;
    type Row = CompatRow<'buf>;
    type BuilderRow = CompatBuilderRow<'buf>;
    type Column = CompatColumn<'buf>;
    type BuilderColumn = CompatColumnBuilder<'buf>;
}

impl<'t, 'b> CellAccessor for CompatRef<'t, 'b> {
    type Target = Cell<'b>;
    type ColName<'l> = Label<'l>;

    fn access(self, pos: usize) -> Option<Self::Target> {
        match self {
            CompatRef::Modern(m) => m.values.get(pos).map(|v| Cell::Single(v.clone())),
            CompatRef::Legacy(l) => l.cells.get(pos).cloned(),
        }
    }
}

impl<'t, 'b> LabelMap for CompatColumnMap<'t, 'b> {
    type Name = Label<'b>;

    fn position(&self, label: &Self::Name) -> Option<usize> {
        match self {
            CompatColumnMap::Modern(m) => m.position(label),
            CompatColumnMap::Legacy(l) => {
                let Label::String(s) = label else { return None };
                l.position(s)
            }
        }
    }
}

impl<'buf> ColumnSerialize for CompatColumn<'buf> {
    fn ser_value_type(&self) -> crate::ValueType {
        self.value_type()
    }

    fn ser_flags(&self) -> &[crate::LegacyFlag] {
        match self {
            Self::Modern(m) => m.ser_flags(),
            Self::Legacy(l) => l.ser_flags(),
        }
    }
}

impl<'a, 'buf> ColumnSerialize for CompatColumnRef<'a, 'buf> {
    fn ser_value_type(&self) -> crate::ValueType {
        self.value_type()
    }

    fn ser_flags(&self) -> &[crate::LegacyFlag] {
        match self {
            Self::Modern(m) => m.ser_flags(),
            Self::Legacy(l) => l.ser_flags(),
        }
    }
}
