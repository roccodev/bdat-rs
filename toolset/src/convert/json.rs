use std::{
    collections::HashMap,
    io::{Read, Write},
};

use anyhow::{anyhow, Context, Result};
use bdat::compat::{CompatColumn, CompatTable};
use bdat::legacy::{LegacyColumn, LegacyColumnBuilder, LegacyRow, LegacyTable, LegacyTableBuilder};
use bdat::modern::{ModernColumn, ModernRow, ModernTable, ModernTableBuilder};
use bdat::{
    serde::{CellSeed, SerializeCell},
    Cell, Label, LegacyFlag, RowId, Value, ValueType,
};
use clap::Args;
use serde::{de::DeserializeSeed, Deserialize, Serialize};
use serde_json::Map;

use crate::error::{FormatError, MAX_DUPLICATE_COLUMNS};
use crate::util::fixed_vec::FixedVec;

use super::{schema::FileSchema, BdatDeserialize, BdatSerialize, ConvertArgs};

#[derive(Args)]
pub struct JsonOptions {
    /// If this is set, JSON output will include spaces and newlines
    /// to improve readability.
    #[arg(long)]
    pretty: bool,
}

#[derive(Serialize, Deserialize)]
struct JsonTable<'b> {
    schema: Option<Vec<ColumnSchema<'b>>>,
    rows: Vec<TableRow>,
}

#[derive(Serialize, Deserialize)]
struct TableRow {
    #[serde(rename = "$id")]
    id: RowId,
    #[serde(flatten)]
    cells: Map<String, serde_json::Value>,
}

#[derive(Deserialize, Serialize)]
struct ColumnSchema<'b> {
    name: String,
    #[serde(rename = "type")]
    ty: ValueType,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    flags: Vec<LegacyFlag<'b>>,
    #[serde(default, skip_serializing_if = "col_skip_count")]
    count: usize,
}

fn col_skip_count(c: &usize) -> bool {
    *c <= 1
}

pub struct JsonConverter {
    untyped: bool,
    pretty: bool,
}

// For duplicate column mitigation
type DuplicateColumnKey<'c> = (FixedVec<usize, MAX_DUPLICATE_COLUMNS>, CompatColumn<'c>);

impl JsonConverter {
    pub fn new(args: &ConvertArgs) -> Self {
        Self {
            untyped: args.untyped,
            pretty: args.json_opts.pretty,
        }
    }

    fn read_table_modern<'b>(&self, name: Label<'b>, table: JsonTable) -> Result<ModernTable<'b>> {
        let schema = table
            .schema
            .ok_or_else(|| FormatError::MissingTypeInfo.with_context(name.clone()))?;

        let (columns, column_map, _): (Vec<ModernColumn>, HashMap<String, (usize, ValueType)>, _) =
            schema.into_iter().try_fold(
                (Vec::new(), HashMap::default(), 0),
                |(mut cols, mut unique_names, idx), col| {
                    let label = Label::parse(col.name.clone(), true);
                    let def = ModernColumn::new(col.ty, label.clone());
                    if unique_names
                        .insert(col.name.clone(), (idx, col.ty))
                        .is_some()
                    {
                        return Err(FormatError::DuplicateColumn(Some(label).into())
                            .with_context(name.clone()));
                    }
                    cols.push(def);
                    Ok((cols, unique_names, idx + 1))
                },
            )?;

        let rows = table
            .rows
            .into_iter()
            .map(|r| {
                let id = r.id;
                let mut values = vec![None; columns.len()];
                for (k, v) in r.cells {
                    let (index, ty) = column_map[&k];
                    values[index] = Some(ty.deser_value(v)?);
                }
                let old_len = values.len();
                let values: Vec<Value> = values.into_iter().flatten().collect();
                if values.len() != old_len {
                    return Err(FormatError::IncompleteRow(id)
                        .with_context(name.clone())
                        .into());
                }
                Ok(ModernRow::new(values))
            })
            .collect::<Result<Vec<_>>>()?;

        Ok(ModernTableBuilder::with_name(name)
            .set_columns(columns)
            .set_rows(rows)
            .build())
    }

    fn read_table_legacy<'b>(
        &self,
        name: Label<'b>,
        table: JsonTable<'b>,
    ) -> Result<LegacyTable<'b>> {
        let schema = table
            .schema
            .ok_or_else(|| FormatError::MissingTypeInfo.with_context(name.clone()))?;

        let Label::String(name_str) = name.clone() else {
            return Err(anyhow!("unsupported table name"));
        };

        let (columns, column_map, _): (Vec<LegacyColumn>, HashMap<String, DuplicateColumnKey>, _) =
            schema.into_iter().try_fold(
                (Vec::new(), HashMap::default(), 0),
                |(mut cols, mut map, idx), col| {
                    let def = LegacyColumnBuilder::new(col.ty, col.name.clone().into())
                        .set_flags(col.flags)
                        .set_count(col.count.max(1))
                        .build();
                    // Only keep the first occurrence: there's a table in XC2 (likely more) with
                    // a duplicate column (FLD_RequestItemSet)
                    let (indices, dup_col) = map
                        .entry(col.name.clone())
                        .or_insert_with(|| (FixedVec::default(), CompatColumn::from(def.clone())));
                    indices.try_push(idx).map_err(|_| {
                        FormatError::MaxDuplicateColumns(Some(Label::from(col.name.clone())).into())
                            .with_context(name.clone())
                    })?;
                    if dup_col.value_type() != col.ty {
                        return Err(FormatError::DuplicateMismatch(Box::new((
                            Some(Label::from(col.name.clone())).into(),
                            dup_col.value_type(),
                            col.ty,
                        )))
                        .with_context(name.clone()));
                    }
                    cols.push(def);
                    Ok((cols, map, idx + 1))
                },
            )?;

        let rows = table
            .rows
            .into_iter()
            .map(|r| {
                let id = r.id;
                let mut cells = vec![None; columns.len()];
                for (k, v) in r.cells {
                    let (index, column) = &column_map[&k];
                    let deserialized = Some(CellSeed::from(column).deserialize(v).unwrap());
                    // Only clone in the worst scenario (duplicate columns)
                    for idx in index.into_iter().skip(1) {
                        cells[*idx] = deserialized.clone();
                    }
                    cells[index[0]] = deserialized;
                }
                let old_len = cells.len();
                let cells: Vec<Cell> = cells.into_iter().flatten().collect();
                if cells.len() != old_len {
                    return Err(FormatError::IncompleteRow(id)
                        .with_context(name.clone())
                        .into());
                }
                Ok(LegacyRow::new(cells))
            })
            .collect::<Result<Vec<_>>>()?;

        Ok(LegacyTableBuilder::with_name(name_str)
            .set_columns(columns)
            .set_rows(rows)
            .build())
    }
}

impl BdatSerialize for JsonConverter {
    fn write_table(&self, table: CompatTable, writer: &mut dyn Write) -> Result<()> {
        let schema = (!self.untyped).then(|| {
            table
                .columns()
                .map(|c| ColumnSchema {
                    name: c.label().to_string(),
                    ty: c.value_type(),
                    flags: c.flags().to_vec(),
                    count: c.count(),
                })
                .collect::<Vec<_>>()
        });

        let columns = table.columns().collect::<Vec<_>>();

        let rows = table
            .rows()
            .map(|row| {
                let cells = columns
                    .iter()
                    .zip(row.cells())
                    .map(|(col, cell)| {
                        (
                            col.label().to_string(),
                            serde_json::to_value(SerializeCell::from_owned(*col, cell)).unwrap(),
                        )
                    })
                    .collect();

                TableRow {
                    id: row.id(),
                    cells,
                }
            })
            .collect::<Vec<_>>();

        let json = JsonTable { schema, rows };
        if self.pretty {
            serde_json::to_writer_pretty(writer, &json)
        } else {
            serde_json::to_writer(writer, &json)
        }
        .context("Failed to write JSON")?;

        Ok(())
    }

    fn get_file_name(&self, table_name: &str) -> String {
        format!("{table_name}.json")
    }
}

impl BdatDeserialize for JsonConverter {
    fn read_table(
        &self,
        name: Label<'static>,
        file_schema: &FileSchema,
        reader: &mut dyn Read,
    ) -> Result<CompatTable> {
        let table: JsonTable =
            serde_json::from_reader(reader).context("failed to read JSON table")?;

        if file_schema.version.is_legacy() {
            self.read_table_legacy(name, table).map(CompatTable::from)
        } else {
            self.read_table_modern(name, table).map(CompatTable::from)
        }
    }

    fn get_table_extension(&self) -> &'static str {
        "json"
    }
}
