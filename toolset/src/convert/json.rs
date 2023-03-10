use std::{
    collections::HashMap,
    io::{Read, Write},
};

use anyhow::{Context, Result};
use bdat::types::{Cell, ColumnDef, Label, Row, Table, TableBuilder, ValueType};
use clap::Args;
use serde::{de::DeserializeSeed, Deserialize, Serialize};
use serde_json::Map;

use crate::error::Error;

use super::{schema::FileSchema, BdatDeserialize, BdatSerialize, ConvertArgs};

#[derive(Args)]
pub struct JsonOptions {
    /// If this is set, JSON output will include spaces and newlines
    /// to improve readability.
    #[arg(long)]
    pretty: bool,
}

#[derive(Serialize, Deserialize)]
struct JsonTable {
    schema: Option<Vec<ColumnSchema>>,
    rows: Vec<TableRow>,
}

#[derive(Serialize, Deserialize)]
struct TableRow {
    #[serde(rename = "$id")]
    id: usize,
    #[serde(flatten)]
    cells: Map<String, serde_json::Value>,
}

#[derive(Deserialize, Serialize)]
struct ColumnSchema {
    name: String,
    #[serde(rename = "type")]
    ty: ValueType,
    hashed: bool,
}

pub struct JsonConverter {
    untyped: bool,
    pretty: bool,
}

impl JsonConverter {
    pub fn new(args: &ConvertArgs) -> Self {
        Self {
            untyped: args.untyped,
            pretty: args.json_opts.pretty,
        }
    }
}

impl BdatSerialize for JsonConverter {
    fn write_table(&self, table: Table, writer: &mut dyn Write) -> Result<()> {
        let schema = (!self.untyped).then(|| {
            table
                .columns()
                .map(|c| ColumnSchema {
                    name: c.label().to_string(),
                    ty: c.value_type(),
                    hashed: matches!(c.label(), Label::Unhashed(_)),
                })
                .collect::<Vec<_>>()
        });

        let columns = table.columns().cloned().collect::<Vec<_>>();

        let rows = table
            .into_rows()
            .map(|row| {
                let id = row.id();
                let cells = columns
                    .iter()
                    .zip(row.cells())
                    .map(|(col, cell)| {
                        (col.label().to_string(), serde_json::to_value(cell).unwrap())
                    })
                    .collect();

                TableRow { id, cells }
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
        name: Option<Label>,
        _schema: &FileSchema,
        reader: &mut dyn Read,
    ) -> Result<Table> {
        let table: JsonTable =
            serde_json::from_reader(reader).context("failed to read JSON table")?;

        let schema = table
            .schema
            .ok_or_else(|| Error::DeserMissingTypeInfo(name.clone().into()))?;

        let (columns, column_map, _): (Vec<ColumnDef>, HashMap<String, (usize, ValueType)>, _) =
            schema.into_iter().fold(
                (Vec::new(), HashMap::default(), 0),
                |(mut cols, mut map, idx), col| {
                    map.insert(col.name.clone(), (idx, col.ty));
                    cols.push(ColumnDef::new(col.ty, Label::parse(col.name, col.hashed)));
                    (cols, map, idx + 1)
                },
            );

        let rows = table
            .rows
            .into_iter()
            .map(|r| {
                let id = r.id;
                let mut cells = vec![None; r.cells.len()];
                for (k, v) in r.cells {
                    let (index, ty) = column_map[&k];
                    cells[index] = Some(ty.as_cell_seed().deserialize(v).unwrap());
                }
                let old_len = cells.len();
                let cells: Vec<Cell> = cells.into_iter().flatten().collect();
                if cells.len() != old_len {
                    return Err(Error::DeserIncompleteRow(id).into());
                }
                Ok(Row::new(id, cells))
            })
            .collect::<Result<Vec<_>>>()?;

        Ok(TableBuilder::new()
            .set_name(name)
            .set_columns(columns)
            .set_rows(rows)
            .build())
    }

    fn get_table_extension(&self) -> &'static str {
        "json"
    }
}
