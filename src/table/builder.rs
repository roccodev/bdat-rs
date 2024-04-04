use crate::ColumnMap;

use super::{
    convert::FormatConvertError,
    legacy::LegacyTable,
    modern::ModernTable,
    private::{Column, Table},
};

pub type ModernTableBuilder<'b> = TableBuilderImpl<'b, ModernTable<'b>>;
pub type LegacyTableBuilder<'b> = TableBuilderImpl<'b, LegacyTable<'b>>;

/// A builder interface for tables.
#[doc(hidden)]
pub struct TableBuilderImpl<'buf, T: Table<'buf>> {
    pub(crate) name: T::Name,
    pub(crate) columns: ColumnMap<T::BuilderColumn, <T::BuilderColumn as Column>::Name>,
    pub(crate) base_id: T::Id,
    pub(crate) rows: Vec<T::BuilderRow>,
}

impl<'b, T> TableBuilderImpl<'b, T>
where
    T: Table<'b>,
{
    pub fn with_name(name: impl Into<T::Name>) -> Self {
        Self {
            name: name.into(),
            base_id: 1.into(), // more sensible default, it's very rare for a table to have 0
            columns: ColumnMap::default(),
            rows: vec![],
        }
    }

    pub(crate) fn from_table(
        name: T::Name,
        base_id: T::Id,
        columns: ColumnMap<T::BuilderColumn, <T::BuilderColumn as Column>::Name>,
        rows: Vec<T::BuilderRow>,
    ) -> Self {
        Self {
            name,
            columns,
            base_id,
            rows,
        }
    }

    pub fn add_column(mut self, column: impl Into<T::BuilderColumn>) -> Self {
        self.columns.push(column.into());
        self
    }

    /// Adds a new row at the end of the table.
    pub fn add_row(mut self, row: impl Into<T::BuilderRow>) -> Self {
        self.rows.push(row.into());
        self
    }

    /// Sets the entire row list for the table.
    pub fn set_rows(mut self, rows: Vec<T::BuilderRow>) -> Self {
        self.rows = rows;
        self
    }

    pub fn set_columns(mut self, columns: impl IntoIterator<Item = T::BuilderColumn>) -> Self {
        self.columns = columns.into_iter().collect();
        self
    }

    pub fn set_base_id(mut self, base_id: T::Id) -> Self {
        self.base_id = base_id;
        self
    }
}

/// Modern builder -> Modern table
impl<'b> ModernTableBuilder<'b> {
    pub fn try_build(self) -> Result<ModernTable<'b>, FormatConvertError> {
        // No need for MaxRowCountExceeded here, we panic on row insertions if
        // the limit is reached, and all legacy table formats have a lower limit
        // than modern tables.
        Ok(ModernTable::new(self))
    }

    pub fn build(self) -> ModernTable<'b> {
        self.try_build().unwrap()
    }
}

/// Legacy builder -> Legacy table
impl<'b> LegacyTableBuilder<'b> {
    pub fn try_build(self) -> Result<LegacyTable<'b>, FormatConvertError> {
        let rows =
            u16::try_from(self.rows.len()).map_err(|_| FormatConvertError::MaxRowCountExceeded)?;
        if self.base_id.checked_add(rows).is_none() {
            // If there are enough rows to overflow from base_id, then we definitely have a row
            // with ID u16::MAX
            return Err(FormatConvertError::UnsupportedRowId(u16::MAX as u32));
        }
        Ok(LegacyTable::new(self))
    }

    pub fn build(self) -> LegacyTable<'b> {
        self.try_build().unwrap()
    }
}
