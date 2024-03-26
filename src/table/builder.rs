use crate::{
    BdatVersion, Cell, ColumnMap, CompatTable, Label, LegacyColumn, LegacyFlag, LegacyTable,
    ModernColumn, ModernTable, ValueType,
};

use super::{
    legacy::LegacyRow,
    modern::ModernRow,
    private::{Column, ColumnSerialize, Table},
    FormatConvertError,
};

pub type CompatTableBuilder<'b> = TableBuilderImpl<'b, CompatTable<'b>>;
pub type ModernTableBuilder<'b> = TableBuilderImpl<'b, ModernTable<'b>>;
pub type LegacyTableBuilder<'b> = TableBuilderImpl<'b, LegacyTable<'b>>;

/// A builder interface for tables.
pub struct TableBuilderImpl<'buf, T: Table<'buf>> {
    pub(crate) name: T::Name,
    pub(crate) columns: ColumnMap<T::BuilderColumn, <T::BuilderColumn as Column>::Name>,
    pub(crate) base_id: T::Id,
    pub(crate) rows: Vec<T::BuilderRow>,
}

pub struct CompatBuilderRow<'b>(Vec<Cell<'b>>);

#[derive(Clone, Debug, PartialEq)]
pub struct CompatColumnBuilder<'buf> {
    value_type: ValueType,
    label: Label<'buf>,
    count: usize,
    flags: Vec<LegacyFlag<'buf>>,
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
            builder.columns.into_iter().map(Into::into).collect(),
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
        let name = builder
            .name
            .try_into()
            .map_err(|_| FormatConvertError::UnsupportedLabelType)?;
        let columns: Result<ColumnMap<_, _>, FormatConvertError> =
            builder.columns.into_iter().map(|c| c.try_into()).collect();
        Ok(Self::from_table(name, base_id, columns?, rows?))
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

    pub fn build(self, version: BdatVersion) -> CompatTable<'b> {
        if version.is_legacy() {
            self.try_into_legacy().unwrap().build().into()
        } else {
            self.try_into_modern().unwrap().build().into()
        }
    }
}

impl<'tb> CompatColumnBuilder<'tb> {
    pub fn new(value_type: ValueType, label: Label<'tb>) -> Self {
        Self {
            value_type,
            label,
            count: 1,
            flags: Vec::new(),
        }
    }

    /// Sets the column's full flag data.
    pub fn set_flags(mut self, flags: Vec<LegacyFlag<'tb>>) -> Self {
        self.flags = flags;
        self
    }

    /// Sets how many elements the column holds, if cells are of the list type.
    pub fn set_count(mut self, count: usize) -> Self {
        assert!(count > 0);
        self.count = count;
        self
    }

    pub fn label(&self) -> &Label {
        &self.label
    }

    pub fn value_type(&self) -> ValueType {
        self.value_type
    }

    pub fn build(self) -> CompatColumnBuilder<'tb> {
        self
    }
}

impl<'b> From<ModernTableBuilder<'b>> for CompatTableBuilder<'b> {
    fn from(builder: ModernTableBuilder<'b>) -> Self {
        Self::from_table(
            builder.name,
            builder.base_id,
            builder
                .columns
                .into_iter()
                .map(CompatColumnBuilder::from)
                .collect(),
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
            builder.name.into(),
            builder.base_id.into(),
            builder
                .columns
                .into_iter()
                .map(CompatColumnBuilder::from)
                .collect(),
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

impl<'buf> From<LegacyColumn<'buf>> for CompatColumnBuilder<'buf> {
    fn from(value: LegacyColumn<'buf>) -> Self {
        Self {
            value_type: value.value_type,
            label: value.label.into(),
            count: value.count,
            flags: value.flags,
        }
    }
}

impl<'buf> From<ModernColumn<'buf>> for CompatColumnBuilder<'buf> {
    fn from(value: ModernColumn<'buf>) -> Self {
        Self {
            value_type: value.value_type,
            label: value.label,
            count: 1,
            flags: Vec::new(),
        }
    }
}

impl<'buf> TryFrom<CompatColumnBuilder<'buf>> for LegacyColumn<'buf> {
    type Error = FormatConvertError;

    fn try_from(value: CompatColumnBuilder<'buf>) -> Result<Self, Self::Error> {
        Ok(Self {
            value_type: value.value_type,
            label: value
                .label
                .try_into()
                .map_err(|_| FormatConvertError::UnsupportedLabelType)?,
            count: value.count,
            flags: value.flags,
        })
    }
}

impl<'buf> From<CompatColumnBuilder<'buf>> for ModernColumn<'buf> {
    fn from(value: CompatColumnBuilder<'buf>) -> Self {
        Self {
            value_type: value.value_type,
            label: value.label,
        }
    }
}

impl<'buf> Column for CompatColumnBuilder<'buf> {
    type Name = Label<'buf>;

    fn clone_label(&self) -> Self::Name {
        self.label.clone()
    }

    fn value_type(&self) -> ValueType {
        self.value_type
    }
}

impl<'buf> ColumnSerialize for CompatColumnBuilder<'buf> {
    fn ser_value_type(&self) -> crate::ValueType {
        self.value_type
    }

    fn ser_flags(&self) -> &[crate::LegacyFlag] {
        &self.flags
    }
}
