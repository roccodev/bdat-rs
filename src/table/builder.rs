use std::marker::PhantomData;

use crate::{
    BdatVersion, Cell, ColumnDef, ColumnMap, Label, LegacyTable, ModernTable, RowId, Table,
};

use super::{legacy::LegacyRow, modern::ModernRow, FormatConvertError};

pub type CompatTableBuilder<'b> = TableBuilderImpl<'b, CompatBuilderRow<'b>, RowId>;
pub type ModernTableBuilder<'b> = TableBuilderImpl<'b, ModernRow<'b>, u32>;
pub type LegacyTableBuilder<'b> = TableBuilderImpl<'b, LegacyRow<'b>, u16>;

/// A builder interface for [`Table`].
pub struct TableBuilderImpl<'b, R: 'b, N> {
    pub(crate) name: Label,
    pub(crate) columns: ColumnMap,
    pub(crate) base_id: N,
    pub(crate) rows: Vec<R>,
    _buf: PhantomData<&'b ()>,
}

pub struct CompatBuilderRow<'b>(Vec<Cell<'b>>);

impl<'b, R: 'b, N> TableBuilderImpl<'b, R, N>
where
    N: From<u8>,
{
    pub fn with_name(name: Label) -> Self {
        Self {
            name,
            base_id: 1.into(), // more sensible default, it's very rare for a table to have 0
            columns: ColumnMap::default(),
            rows: vec![],
            _buf: PhantomData,
        }
    }

    pub(crate) fn from_table(name: Label, base_id: N, columns: ColumnMap, rows: Vec<R>) -> Self {
        Self {
            name,
            columns,
            base_id,
            rows,
            _buf: PhantomData,
        }
    }

    pub fn add_column(mut self, column: ColumnDef) -> Self {
        self.columns.push(column);
        self
    }

    /// Adds a new row at the end of the table.
    pub fn add_row(mut self, row: R) -> Self {
        self.rows.push(row);
        self
    }

    /// Sets the entire row list for the table.
    pub fn set_rows(mut self, rows: Vec<R>) -> Self {
        self.rows = rows;
        self
    }

    pub fn set_columns(mut self, columns: Vec<ColumnDef>) -> Self {
        self.columns = ColumnMap::from(columns);
        self
    }

    pub fn set_base_id(mut self, base_id: N) -> Self {
        self.base_id = base_id;
        self
    }
}

/// Modern builder -> Modern table
impl<'b> ModernTableBuilder<'b> {
    fn from_compat(builder: CompatTableBuilder<'b>) -> Result<Self, FormatConvertError> {
        if let Some(col) = builder
            .columns
            .iter()
            .find(|c| !c.value_type().is_supported(BdatVersion::Modern))
        {
            return Err(FormatConvertError::UnsupportedValueType(col.value_type()));
        }
        let rows: Result<Vec<_>, FormatConvertError> = builder
            .rows
            .into_iter()
            .map(|r| r.try_into_modern())
            .collect();
        Ok(Self::from_table(
            builder.name,
            builder.base_id,
            builder.columns,
            rows?,
        ))
    }

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
    fn from_compat(
        builder: CompatTableBuilder<'b>,
    ) -> Result<LegacyTableBuilder<'b>, FormatConvertError> {
        // any legacy version works here
        if let Some(col) = builder
            .columns
            .iter()
            .find(|c| !c.value_type().is_supported(BdatVersion::LegacySwitch))
        {
            return Err(FormatConvertError::UnsupportedValueType(col.value_type()));
        }
        let rows: Result<Vec<_>, FormatConvertError> = builder
            .rows
            .into_iter()
            .map(CompatBuilderRow::try_into_legacy)
            .collect();
        let base_id = u16::try_from(builder.base_id)
            .map_err(|_| FormatConvertError::UnsupportedRowId(builder.base_id))?;
        Ok(Self::from_table(
            builder.name,
            base_id,
            builder.columns,
            rows?,
        ))
    }

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

impl<'b> CompatBuilderRow<'b> {
    pub fn try_into_modern(self) -> Result<ModernRow<'b>, FormatConvertError> {
        let rows: Result<Vec<_>, FormatConvertError> = self
            .0
            .into_iter()
            .map(|c| match c {
                Cell::Single(v) => Ok(v),
                _ => Err(FormatConvertError::UnsupportedCell),
            })
            .collect();
        rows.map(ModernRow::new)
    }

    pub fn try_into_legacy(self) -> Result<LegacyRow<'b>, FormatConvertError> {
        Ok(LegacyRow::new(self.0))
    }
}

/// Compat builder -> Compat table
impl<'b> CompatTableBuilder<'b> {
    pub fn try_into_legacy(self) -> Result<LegacyTableBuilder<'b>, FormatConvertError> {
        LegacyTableBuilder::from_compat(self)
    }

    pub fn try_into_modern(self) -> Result<ModernTableBuilder<'b>, FormatConvertError> {
        ModernTableBuilder::from_compat(self)
    }

    pub fn build(self, version: BdatVersion) -> Table<'b> {
        if version.is_legacy() {
            self.try_into_legacy().unwrap().build().into()
        } else {
            self.try_into_modern().unwrap().build().into()
        }
    }
}

impl<'b> From<ModernTableBuilder<'b>> for CompatTableBuilder<'b> {
    fn from(builder: ModernTableBuilder<'b>) -> Self {
        Self::from_table(
            builder.name,
            builder.base_id,
            builder.columns,
            builder
                .rows
                .into_iter()
                .map(|r| {
                    CompatBuilderRow::from(r.into_values().map(Cell::Single).collect::<Vec<_>>())
                })
                .collect(),
        )
    }
}

impl<'b> From<LegacyTableBuilder<'b>> for CompatTableBuilder<'b> {
    fn from(builder: LegacyTableBuilder<'b>) -> Self {
        Self::from_table(
            builder.name,
            builder.base_id.into(),
            builder.columns,
            builder
                .rows
                .into_iter()
                .map(|r| CompatBuilderRow::from(r.cells))
                .collect(),
        )
    }
}

impl<'b> TryFrom<CompatTableBuilder<'b>> for ModernTableBuilder<'b> {
    type Error = FormatConvertError;

    fn try_from(builder: CompatTableBuilder<'b>) -> Result<Self, Self::Error> {
        builder.try_into_modern()
    }
}

impl<'b> From<Vec<Cell<'b>>> for CompatBuilderRow<'b> {
    fn from(value: Vec<Cell<'b>>) -> Self {
        Self(value)
    }
}
