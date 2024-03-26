use thiserror::Error;

use crate::{
    BdatVersion, Cell, ColumnMap, LegacyColumn, LegacyRow, LegacyTable, LegacyTableBuilder,
    ModernColumn, ModernRow, ModernTable, ModernTableBuilder, RowId, ValueType,
};

/// Error encountered while converting tables
/// to a different format.
#[derive(Error, Debug)]
pub enum FormatConvertError {
    /// One of the table's columns has an unsupported value type.
    ///
    /// For example, legacy tables do not support hash-ref fields.
    #[error("unsupported value type {0:?}")]
    UnsupportedValueType(ValueType),
    /// One of the table's values has an unsupported cell type.
    ///
    /// For instance, modern tables only support single-value cells.
    #[error("unsupported cell")]
    UnsupportedCell,
    /// The max number of rows in the table has been reached, so no
    /// more rows can be added.
    #[error("max row count exceeded")]
    MaxRowCountExceeded,
    /// The destination format (legacy) does not support the row ID because it is too high.
    /// This can happen if the base ID or any of the rows's ID is outside of the format's
    /// row ID boundaries.
    #[error("unsupported row ID {0}")]
    UnsupportedRowId(RowId),
    /// The destination format does not support hashed labels.
    #[error("unsupported label type")]
    UnsupportedLabelType,
}

// Modern table -> Legacy table

impl<'b> TryFrom<ModernColumn<'b>> for LegacyColumn<'b> {
    type Error = FormatConvertError;

    fn try_from(modern_col: ModernColumn<'b>) -> Result<Self, Self::Error> {
        // any legacy version works here
        if !modern_col
            .value_type()
            .is_supported(BdatVersion::LegacySwitch)
        {
            return Err(FormatConvertError::UnsupportedValueType(
                modern_col.value_type(),
            ));
        }
        Ok(Self {
            value_type: modern_col.value_type,
            label: modern_col
                .label
                .try_into()
                .map_err(|_| FormatConvertError::UnsupportedLabelType)?,
            count: 1,
            flags: Vec::new(),
        })
    }
}

impl<'b> From<ModernRow<'b>> for LegacyRow<'b> {
    fn from(value: ModernRow<'b>) -> Self {
        Self {
            cells: value.values.into_iter().map(Cell::Single).collect(),
        }
    }
}

impl<'b> TryFrom<ModernTable<'b>> for LegacyTable<'b> {
    type Error = FormatConvertError;

    fn try_from(modern_table: ModernTable<'b>) -> Result<Self, Self::Error> {
        let rows: Vec<_> = modern_table.rows.into_iter().map(Into::into).collect();
        let base_id = u16::try_from(modern_table.base_id)
            .map_err(|_| FormatConvertError::UnsupportedRowId(modern_table.base_id))?;
        let name = modern_table
            .name
            .try_into()
            .map_err(|_| FormatConvertError::UnsupportedLabelType)?;
        let columns: Result<ColumnMap<_, _>, FormatConvertError> = modern_table
            .columns
            .into_iter()
            .map(TryInto::try_into)
            .collect();
        let row_len =
            u16::try_from(rows.len()).map_err(|_| FormatConvertError::MaxRowCountExceeded)?;
        if base_id.checked_add(row_len).is_none() {
            // If there are enough rows to overflow from base_id, then we definitely have a row
            // with ID u16::MAX
            return Err(FormatConvertError::UnsupportedRowId(u16::MAX as u32));
        }
        Ok(LegacyTableBuilder::from_table(name, base_id, columns?, rows).build())
    }
}

// Legacy table -> Modern table

impl<'b> TryFrom<LegacyColumn<'b>> for ModernColumn<'b> {
    type Error = FormatConvertError;

    fn try_from(legacy_col: LegacyColumn<'b>) -> Result<Self, Self::Error> {
        if !legacy_col.value_type().is_supported(BdatVersion::Modern) {
            return Err(FormatConvertError::UnsupportedValueType(
                legacy_col.value_type(),
            ));
        }
        Ok(Self {
            value_type: legacy_col.value_type,
            label: legacy_col.label.into(),
        })
    }
}

impl<'b> TryFrom<LegacyRow<'b>> for ModernRow<'b> {
    type Error = FormatConvertError;

    fn try_from(legacy_row: LegacyRow<'b>) -> Result<Self, Self::Error> {
        let values: Result<Vec<_>, FormatConvertError> = legacy_row
            .into_cells()
            .map(|c| match c {
                Cell::Single(v) => Ok(v),
                _ => Err(FormatConvertError::UnsupportedCell),
            })
            .collect();
        Ok(Self { values: values? })
    }
}

impl<'b> TryFrom<LegacyTable<'b>> for ModernTable<'b> {
    type Error = FormatConvertError;

    fn try_from(legacy_table: LegacyTable<'b>) -> Result<Self, Self::Error> {
        let columns: Result<ColumnMap<_>, FormatConvertError> = legacy_table
            .columns
            .into_iter()
            .map(TryInto::try_into)
            .collect();
        let rows: Result<Vec<_>, FormatConvertError> = legacy_table
            .rows
            .into_iter()
            .map(TryInto::try_into)
            .collect();

        Ok(ModernTableBuilder::from_table(
            legacy_table.name.into(),
            legacy_table.base_id as u32,
            columns?,
            rows?,
        )
        .build())
    }
}
