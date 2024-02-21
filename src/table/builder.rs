use std::marker::PhantomData;

use crate::{
    BdatVersion, Cell, ColumnDef, ColumnMap, Label, LegacyTable, ModernTable, RowId, Table,
};

use super::{legacy::LegacyRow, modern::ModernRow, FormatConvertError};

pub type TableBuilder<'b> = TableBuilderImpl<'b, CompatBuilderRow<'b>>;
pub type ModernTableBuilder<'b> = TableBuilderImpl<'b, ModernRow<'b>>;
pub type LegacyTableBuilder<'b> = TableBuilderImpl<'b, LegacyRow<'b>>;

/// A builder interface for [`Table`].
pub struct TableBuilderImpl<'b, R: 'b> {
    pub(crate) name: Label,
    pub(crate) columns: ColumnMap,
    pub(crate) base_id: RowId,
    pub(crate) rows: Vec<R>,
    _buf: PhantomData<&'b ()>,
}

pub struct CompatBuilderRow<'b>(Vec<Cell<'b>>);

impl<'b, R: 'b> TableBuilderImpl<'b, R> {
    pub fn with_name(name: Label) -> Self {
        Self {
            name,
            base_id: 1, // more sensible default, it's very rare for a table to have 0
            columns: ColumnMap::default(),
            rows: vec![],
            _buf: PhantomData,
        }
    }

    pub(crate) fn from_table(
        name: Label,
        base_id: RowId,
        columns: ColumnMap,
        rows: Vec<R>,
    ) -> Self {
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

    pub fn set_base_id(mut self, base_id: RowId) -> Self {
        self.base_id = base_id;
        self
    }
}

/// Modern builder -> Modern table
impl<'b> TableBuilderImpl<'b, ModernRow<'b>> {
    fn from_compat(builder: TableBuilder<'b>) -> Result<Self, FormatConvertError> {
        if let Some(col) = builder
            .columns
            .iter()
            .find(|c| !c.value_type().is_supported(BdatVersion::Modern))
        {
            return Err(FormatConvertError::UnsupportedValueType(col.value_type()));
        }
        let rows: Result<Vec<_>, FormatConvertError> =
            builder.rows.into_iter().map(|r| r.to_modern()).collect();
        Ok(Self::from_table(
            builder.name,
            builder.base_id as u32,
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
impl<'b> TableBuilderImpl<'b, LegacyRow<'b>> {
    fn from_compat(
        builder: TableBuilder<'b>,
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
            .map(CompatBuilderRow::to_legacy)
            .collect();
        Ok(Self::from_table(
            builder.name,
            builder.base_id,
            builder.columns,
            rows?,
        ))
    }

    pub fn try_build(self) -> Result<LegacyTable<'b>, FormatConvertError> {
        if self.rows.len() >= u16::MAX as usize {
            return Err(FormatConvertError::MaxRowCountExceeded);
        }
        // TODO check base id
        Ok(LegacyTable::new(self))
    }

    pub fn build(self) -> LegacyTable<'b> {
        self.try_build().unwrap()
    }
}

impl<'b> CompatBuilderRow<'b> {
    pub fn to_modern(self) -> Result<ModernRow<'b>, FormatConvertError> {
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

    pub fn to_legacy(self) -> Result<LegacyRow<'b>, FormatConvertError> {
        Ok(LegacyRow::new(self.0))
    }
}

/// Compat builder -> Compat table
impl<'b> TableBuilderImpl<'b, CompatBuilderRow<'b>> {
    pub fn to_legacy(self) -> Result<LegacyTableBuilder<'b>, FormatConvertError> {
        LegacyTableBuilder::from_compat(self)
    }

    pub fn to_modern(self) -> Result<ModernTableBuilder<'b>, FormatConvertError> {
        ModernTableBuilder::from_compat(self)
    }

    pub fn build(self, version: BdatVersion) -> Table<'b> {
        if version.is_legacy() {
            self.to_legacy().unwrap().build().into()
        } else {
            self.to_modern().unwrap().build().into()
        }
    }
}

impl<'b> From<ModernTableBuilder<'b>> for TableBuilder<'b> {
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

impl<'b> From<LegacyTableBuilder<'b>> for TableBuilder<'b> {
    fn from(builder: LegacyTableBuilder<'b>) -> Self {
        Self::from_table(
            builder.name,
            builder.base_id,
            builder.columns,
            builder
                .rows
                .into_iter()
                .map(|r| CompatBuilderRow::from(r.cells))
                .collect(),
        )
    }
}

impl<'b> TryFrom<TableBuilder<'b>> for ModernTableBuilder<'b> {
    type Error = FormatConvertError;

    fn try_from(builder: TableBuilder<'b>) -> Result<Self, Self::Error> {
        builder.to_modern()
    }
}

impl<'b> From<Vec<Cell<'b>>> for CompatBuilderRow<'b> {
    fn from(value: Vec<Cell<'b>>) -> Self {
        Self(value)
    }
}
