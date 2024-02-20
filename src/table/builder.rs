use std::marker::PhantomData;

use crate::{
    BdatResult, BdatVersion, Cell, ColumnDef, ColumnMap, Label, ModernTable, LegacyTable, Table, RowId
};

use super::{modern::ModernRow, legacy::LegacyRow, compat::CompatRow};

pub type TableBuilder<'b> = TableBuilderImpl<'b, CompatRow<'b>>;
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

impl<'b, R: 'b> TableBuilderImpl<'b, R> {
    pub fn with_name(name: Label) -> Self {
        Self {
            name,
            base_id: 0,
            columns: ColumnMap::default(),
            rows: vec![],
            _buf: PhantomData,
        }
    }

    pub(crate) fn from_table(name: Label, base_id: RowId, columns: ColumnMap, rows: Vec<R>) -> Self {
        Self { name, columns, base_id, rows, _buf: PhantomData }
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
    fn from_compat(builder: TableBuilder<'b>) -> Self {
        Self::from_table(builder.name, builder.base_id, builder.columns, builder.rows.into_iter()
                .map(CompatRow::to_modern)
                .collect())
    }

    pub fn build(self) -> ModernTable<'b> {
        ModernTable::new(self)
    }
}

/// Legacy builder -> Legacy table
impl<'b> TableBuilderImpl<'b, LegacyRow<'b>> {
    fn from_compat(builder: TableBuilder<'b>) -> Self {
        Self::from_table(builder.name, builder.base_id, builder.columns, builder.rows.into_iter()
            .map(CompatRow::to_legacy)
            .collect())
    }

    pub fn build(self) -> LegacyTable<'b> {
        assert!(self.rows.len() < u16::MAX as usize, "legacy tables only allow up to {} rows", u16::MAX);
        // TODO check base id
        LegacyTable::new(self)
    }
}

/// Compat builder -> Compat table
impl<'b> TableBuilderImpl<'b, CompatRow<'b>> {
    pub fn to_legacy(self) -> LegacyTableBuilder<'b> {
        LegacyTableBuilder::from_compat(self)
    }

    pub fn to_modern(self) -> ModernTableBuilder<'b> {
        ModernTableBuilder::from_compat(self)
    }

    pub fn build(self, version: BdatVersion) -> Table<'b> {
        if version.is_legacy() {
            self.to_legacy().build().into()
        } else {
            self.to_modern().build().into()
        }
    }
}
