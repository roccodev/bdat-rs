use std::{
    collections::HashMap,
    io::{Read, Write},
};

use anyhow::{Context, Result};
use bdat::types::{Cell, Label, RawTable, ValueType};
use clap::Args;
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

use super::{schema::FileSchema, BdatDeserialize, BdatSerialize, ConvertArgs};

#[derive(Args)]
pub struct JsonOptions {
    /// If this is set, JSON output will include spaces and newlines
    /// to improve readability.
    #[arg(long)]
    pretty: bool,
}

#[derive(Serialize)]
struct JsonTable {
    schema: Option<Vec<ColumnSchema>>,
    rows: Vec<TableRow>,
}

#[derive(Serialize)]
struct TableRow {
    #[serde(rename = "$id")]
    id: usize,
    #[serde(flatten)]
    cells: HashMap<String, Cell>,
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

    fn convert(&self, bdat: bdat::types::Value) -> Value {
        serde_json::to_value(bdat).unwrap()
    }
}

impl BdatSerialize for JsonConverter {
    fn write_table(&self, table: RawTable, writer: &mut dyn Write) -> Result<()> {
        let schema = (!self.untyped).then(|| {
            table
                .columns
                .iter()
                .map(|c| ColumnSchema {
                    name: c.label.to_string(),
                    ty: c.ty,
                    hashed: matches!(c.label, Label::Unhashed(_)),
                })
                .collect::<Vec<_>>()
        });

        let rows = table
            .rows
            .into_iter()
            .map(|mut row| {
                let cells = table
                    .columns
                    .iter()
                    .map(|col| (col.label.to_string(), row.cells.remove(0)))
                    .collect();

                TableRow { id: row.id, cells }
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
    fn read_table(&self, schema: &FileSchema, reader: &mut dyn Read) -> Result<RawTable> {
        //let table: JsonTable =
        //    serde_json::from_reader(reader).context("failed to read JSON table")?;
        Ok(todo!())
    }
}
