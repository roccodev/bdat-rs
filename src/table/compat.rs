//! Adapters for version-agnostic BDAT tables.
//!
//! If a file or table's version is known in advance, the
//! versioned modules [`modern`] and [`legacy`] should be preferred.
//!
//! [`modern`]: crate::modern
//! [`legacy`]: crate::legacy

use std::convert::Infallible;

use super::legacy::LegacyRow;
use super::modern::ModernRow;
use super::private::{CellAccessor, ColumnSerialize, LabelMap, Table};
use super::util::CompatIter;
use crate::{
    BdatResult, Cell, ColumnMap, Label, LegacyColumn, LegacyFlag, LegacyTable, ModernColumn,
    ModernTable, RowId, RowRef, Utf, ValueType,
};

/// A BDAT table view with version metadata.
///
/// This compatibility wrapper allows users to query table information independent of its version,
/// and also perform basic queries on rows.
///
/// This, however, introduces several limitations. For instance, some operations may fail or panic
/// due to being unsupported on either version. Additionally, some operations incur extra overhead
/// as they need to wrap the result, sometimes cloning to take ownership of it.
///
/// Modifications can only be performed on versioned tables. You can `match` on this enum to get
/// the versioned representation, though methods like [`as_modern_mut`] and [`as_legacy_mut`] are
/// also provided, if the type is known in advance.
///
/// New tables **must** be built as versioned tables. In other words, there is no builder for
/// this compatibility wrapper, you must use one of [`LegacyTableBuilder`] or [`ModernTableBuilder`].
/// You may then wrap the build result if you deem it necessary.
///
/// See also the [module-level documentation](crate::table) for tables.
///
/// ## Examples
///
/// ```
/// # use bdat::*;
/// # fn read(bytes: &mut [u8]) -> BdatResult<()> {
/// let table: &CompatTable = &bdat::from_bytes(bytes)?.get_tables()?[0];
/// println!("Table {} has {} rows.", table.name(), table.row_count());
/// # Ok(())
/// # }
/// ```
///
/// [`as_modern_mut`]: CompatTable::as_modern_mut
/// [`as_legacy_mut`]: CompatTable::as_legacy_mut
/// [`LegacyTableBuilder`]: crate::LegacyTableBuilder
/// [`ModernTableBuilder`]: crate::ModernTableBuilder
#[derive(Debug, Clone, PartialEq)]
pub enum CompatTable<'b> {
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

#[derive(Clone, PartialEq, Eq)]
pub enum CompatColumn<'buf> {
    Modern(ModernColumn<'buf>),
    Legacy(LegacyColumn<'buf>),
}

#[derive(Clone, Copy)]
pub enum CompatColumnRef<'t, 'buf> {
    Modern(&'t ModernColumn<'buf>),
    Legacy(&'t LegacyColumn<'buf>),
}

#[derive(Clone, Copy)]
pub enum CompatColumnMap<'t, 'buf> {
    Modern(&'t ColumnMap<ModernColumn<'buf>, Label<'buf>>),
    Legacy(&'t ColumnMap<LegacyColumn<'buf>, Utf<'buf>>),
}

pub type CompatRowRef<'t, 'buf> = RowRef<CompatRef<'t, 'buf>, CompatColumnMap<'t, 'buf>>;

macro_rules! versioned {
    ($var:expr, $name:ident) => {
        match $var {
            Self::Modern(m) => &m.$name,
            Self::Legacy(l) => &l.$name,
        }
    };
    ($var:expr, $name:ident($($par:expr ) *)) => {
        match $var {
            Self::Modern(m) => m . $name ( $($par, )* ),
            Self::Legacy(l) => l . $name ( $($par, )* ),
        }
    };
}

impl<'b> CompatTable<'b> {
    /// If the table is modern, returns a view of the underlying table.
    ///
    /// ## Panics
    /// Panics if the table is not modern.
    pub fn as_modern(&self) -> &ModernTable<'b> {
        match self {
            Self::Modern(m) => m,
            _ => panic!("not modern"),
        }
    }

    /// If the table is legacy, returns a view of the underlying table.
    ///
    /// ## Panics
    /// Panics if the table is not legacy.
    pub fn as_legacy(&self) -> &LegacyTable<'b> {
        match self {
            Self::Legacy(l) => l,
            _ => panic!("not legacy"),
        }
    }

    /// If the table is modern, returns a mutable view of the underlying table.
    ///
    /// ## Panics
    /// Panics if the table is not modern.
    pub fn as_modern_mut(&mut self) -> &mut ModernTable<'b> {
        match self {
            Self::Modern(m) => m,
            _ => panic!("not modern"),
        }
    }

    /// If the table is legacy, returns a mutable view of the underlying table.
    ///
    /// ## Panics
    /// Panics if the table is not legacy.
    pub fn as_legacy_mut(&mut self) -> &mut LegacyTable<'b> {
        match self {
            Self::Legacy(l) => l,
            _ => panic!("not legacy"),
        }
    }

    /// If the table is modern, returns the underlying table.
    ///
    /// ## Panics
    /// Panics if the table is not modern.  
    /// For a panic-free function that converts instead, use [`try_into_modern`].
    ///
    /// [`try_into_modern`]: Self::try_into_modern
    pub fn into_modern(self) -> ModernTable<'b> {
        match self {
            Self::Modern(m) => m,
            _ => panic!("not modern"),
        }
    }

    /// If the table is legacy, returns the underlying table.
    ///
    /// ## Panics
    /// Panics if the table is not legacy.  
    /// For a panic-free function that converts instead, use [`try_into_legacy`].
    ///
    /// [`try_into_legacy`]: Self::try_into_legacy
    pub fn into_legacy(self) -> LegacyTable<'b> {
        match self {
            Self::Legacy(l) => l,
            _ => panic!("not legacy"),
        }
    }

    /// Returns whether the underlying table is modern.
    pub fn is_modern(&self) -> bool {
        matches!(self, Self::Modern(_))
    }

    /// Returns whether the underlying table is legacy.
    pub fn is_legacy(&self) -> bool {
        matches!(self, Self::Legacy(_))
    }

    /// Returns a modern table as close to the underlying table as possible.
    ///
    /// * If the table is modern, this does nothing and returns it.
    /// * If the table is legacy, it tries to convert it to the
    /// modern format, and returns the result.
    pub fn try_into_modern(self) -> BdatResult<ModernTable<'b>> {
        match self {
            Self::Modern(m) => Ok(m),
            Self::Legacy(l) => Ok(l.try_into()?),
        }
    }

    /// Returns a legacy table as close to the underlying table as possible.
    ///
    /// * If the table is legacy, this does nothing and returns it.
    /// * If the table is modern, it tries to convert it to the
    /// legacy format, and returns the result.
    pub fn try_into_legacy(self) -> BdatResult<LegacyTable<'b>> {
        match self {
            Self::Modern(m) => Ok(m.try_into()?),
            Self::Legacy(l) => Ok(l),
        }
    }

    /// Returns the table's name. For legacy tables, this is wrapped
    /// into a [`Label::String`].
    pub fn name(&self) -> Label {
        match self {
            Self::Modern(m) => m.name().as_ref(),
            Self::Legacy(l) => l.name().into(),
        }
    }

    pub(crate) fn name_cloned(&self) -> Label<'b> {
        match self {
            Self::Modern(m) => m.name.clone(),
            Self::Legacy(l) => l.name.clone().into(),
        }
    }

    /// Changes the table's name.
    ///
    /// ## Panics
    /// Panics if `name` is a label that is unsupported by the destination
    /// format, e.g. hashed labels in legacy tables.
    pub fn set_name(&mut self, name: Label<'b>) {
        match self {
            Self::Modern(m) => m.set_name(name),
            Self::Legacy(l) => {
                l.set_name(name.try_into().expect("hashed labels are not supported"))
            }
        }
    }

    /// Gets the minimum row ID in the table.
    pub fn base_id(&self) -> RowId {
        match self {
            Self::Modern(m) => m.base_id(),
            Self::Legacy(l) => l.base_id() as u32,
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
        match self {
            Self::Modern(m) => m
                .row(id)
                .map(CompatRef::Modern, CompatColumnMap::Modern(&m.columns)),
            Self::Legacy(l) => l
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
        match self {
            Self::Modern(m) => m
                .get_row(id)
                .map(|r| r.map(CompatRef::Modern, CompatColumnMap::Modern(&m.columns))),
            Self::Legacy(l) => id
                .try_into()
                .ok()
                .and_then(|id| l.get_row(id))
                .map(|r| r.map(CompatRef::Legacy, CompatColumnMap::Legacy(&l.columns))),
        }
    }

    /// Gets an iterator that visits this table's rows
    pub fn rows(&self) -> impl Iterator<Item = CompatRowRef<'_, 'b>> {
        match self {
            Self::Modern(m) => CompatIter::Modern(
                m.rows()
                    .map(|r| r.map(CompatRef::Modern, CompatColumnMap::Modern(&m.columns))),
            ),
            Self::Legacy(l) => CompatIter::Legacy(
                l.rows()
                    .map(|r| r.map(CompatRef::Legacy, CompatColumnMap::Legacy(&l.columns))),
            ),
        }
    }

    /// Gets an owning iterator over this table's rows
    pub fn into_rows(self) -> impl Iterator<Item = CompatRow<'b>> {
        match self {
            Self::Modern(m) => CompatIter::Modern(m.into_rows().map(CompatRow::Modern)),
            Self::Legacy(l) => CompatIter::Legacy(l.into_rows().map(CompatRow::Legacy)),
        }
    }

    /// Gets an owning iterator over this table's rows, in pairs of
    /// `(row ID, row)`.
    pub fn into_rows_id(self) -> impl Iterator<Item = (u32, CompatRow<'b>)> {
        match self {
            Self::Modern(m) => {
                CompatIter::Modern(m.into_rows_id().map(|(id, r)| (id, CompatRow::Modern(r))))
            }
            Self::Legacy(l) => CompatIter::Legacy(
                l.into_rows_id()
                    .map(|(id, r)| (id as u32, CompatRow::Legacy(r))),
            ),
        }
    }

    /// Gets an iterator that visits this table's column definitions
    pub fn columns(&self) -> impl Iterator<Item = CompatColumnRef<'_, 'b>> {
        match self {
            Self::Modern(m) => CompatIter::Modern(m.columns().map(CompatColumnRef::Modern)),
            Self::Legacy(l) => CompatIter::Legacy(l.columns().map(CompatColumnRef::Legacy)),
        }
    }

    /// Gets an owning iterator over this table's column definitions.
    ///
    /// Columns from modern tables will be returned as-is. In the case of legacy
    /// tables, column names are wrapped into the [`Label`] type.
    pub fn into_columns(self) -> impl Iterator<Item = CompatColumn<'b>> {
        match self {
            Self::Modern(m) => CompatIter::Modern(m.into_columns().map(CompatColumn::Modern)),
            Self::Legacy(l) => CompatIter::Legacy(l.into_columns().map(CompatColumn::Legacy)),
        }
    }

    pub fn row_count(&self) -> usize {
        versioned!(&self, row_count())
    }

    pub fn column_count(&self) -> usize {
        versioned!(&self, column_count())
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
    /// Returns the column's label. For legacy tables,
    /// this is wrapped into a [`Label::String`].
    pub fn label(&self) -> Label {
        match self {
            Self::Modern(m) => m.label().as_ref(),
            Self::Legacy(l) => l.label().into(),
        }
    }

    pub fn value_type(&self) -> ValueType {
        self.as_ref().value_type()
    }

    /// Returns the column's list of defined flags.
    ///
    /// For modern tables this always returns `&[]`.
    pub fn flags(&self) -> &[LegacyFlag<'buf>] {
        match self {
            Self::Modern(_) => &[],
            Self::Legacy(l) => l.flags(),
        }
    }

    /// Returns the number of values in a cell of this column.
    ///
    /// For modern tables and non-array cells, this returns 1.
    pub fn count(&self) -> usize {
        self.as_ref().count()
    }

    /// Returns the total data size that a single cell of this column
    /// holds.
    ///
    /// For modern tables, this is always the size of the value type.
    pub fn data_size(&self) -> usize {
        self.as_ref().data_size()
    }
}

impl<'t, 'buf> CompatColumnRef<'t, 'buf> {
    /// Returns the column's label. For legacy tables,
    /// this is wrapped into a [`Label::String`].
    pub fn label(&self) -> Label<'t> {
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

    /// Returns the column's list of defined flags.
    ///
    /// For modern tables this always returns `&[]`.
    pub fn flags(&self) -> &[LegacyFlag<'buf>] {
        match self {
            Self::Modern(_) => &[],
            Self::Legacy(l) => l.flags(),
        }
    }

    /// Returns the number of values in a cell of this column.
    ///
    /// For modern tables and non-array cells, this returns 1.
    pub fn count(&self) -> usize {
        match self {
            Self::Modern(_) => 1,
            Self::Legacy(l) => l.count(),
        }
    }

    /// Returns the total data size that a single cell of this column
    /// holds.
    ///
    /// For modern tables, this is always the size of the value type.
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

impl<'buf> From<LegacyColumn<'buf>> for CompatColumn<'buf> {
    fn from(value: LegacyColumn<'buf>) -> Self {
        Self::Legacy(value)
    }
}

impl<'buf> From<ModernColumn<'buf>> for CompatColumn<'buf> {
    fn from(value: ModernColumn<'buf>) -> Self {
        Self::Modern(value)
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
    type BuilderRow = Infallible; // uninstantiable
    type Column = CompatColumn<'buf>;
    type BuilderColumn = CompatColumn<'buf>;
}

impl<'t, 'b> CellAccessor for CompatRef<'t, 'b> {
    type Target = Cell<'b>;

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
