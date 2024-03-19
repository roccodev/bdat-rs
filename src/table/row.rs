use crate::LabelMap;
use std::ops::{Deref, DerefMut};

/// Best-fit type for row IDs.
/// In legacy BDATs, row identifiers are 16-bit.
/// In modern BDATs, row IDs are 32-bit.
pub type RowId = u32;

/// A reference to a row that also keeps information about the parent table.
///
/// ## Accessing cells
/// Accessing cells from a `RowRef` is very easy:
///
/// ```
/// use bdat::{RowRef, ModernTable};
///
/// fn param_1(table: ModernTable) -> u32 {
///     let row = table.row(1);
///     // Use .get() to access cells
///     row.get("Param1").to_integer()
/// }
///
/// fn param_2_if_present(table: ModernTable) -> Option<u32> {
///     let row = table.row(1);
///     // Or use .get_if_present() for columns that might be absent
///     row.get_if_present("Param2").map(|value| value.to_integer())
/// }
/// ```
#[derive(Clone, Copy, Debug)]
pub struct RowRef<R, L> {
    id: RowId,
    row: R,
    columns: L,
}

pub trait CellAccessor {
    type Target;
    type ColName<'n>: PartialEq;

    fn access(self, pos: usize) -> Option<Self::Target>;
}

impl<R, L> RowRef<R, L>
where
    R: CellAccessor,
    L: LabelMap,
{
    pub(crate) fn new(id: RowId, row: R, label_map: L) -> Self {
        Self {
            id,
            row,
            columns: label_map,
        }
    }

    pub(crate) fn map<O: CellAccessor, M: LabelMap>(
        self,
        mapper: impl FnOnce(R) -> O,
        columns: M,
    ) -> RowRef<O, M> {
        RowRef {
            id: self.id,
            row: mapper(self.row),
            columns,
        }
    }

    pub fn id(&self) -> RowId {
        self.id
    }

    /// Returns a reference to the cell at the given column.
    ///
    /// If there is no column with the given label, this returns [`None`].
    pub fn get_if_present(self, column: impl Into<L::Name>) -> Option<R::Target> {
        let index = self.columns.position(&column.into())?;
        self.row.access(index)
    }

    /// Returns a reference to the cell at the given column.
    ///
    /// ## Panics
    /// Panics if there is no column with the given label.
    pub fn get(self, column: impl Into<L::Name>) -> R::Target {
        self.get_if_present(column).expect("no such column")
    }
}

impl<R, L> Deref for RowRef<R, L> {
    type Target = R;

    fn deref(&self) -> &Self::Target {
        &self.row
    }
}

impl<R, L> DerefMut for RowRef<R, L> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.row
    }
}
