use std::io::Write;

use anyhow::{Context, Result};
use bdat::types::{Cell, Label, RawTable};
use clap::Args;
use serde_json::{json, Map, Value};

use super::{BdatSerialize, ConvertArgs};

#[derive(Args)]
pub struct JsonOptions {
    /// If this is set, JSON output will include spaces and newlines
    /// to improve readability.
    #[arg(long)]
    pretty: bool,
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
        use bdat::types::Value as Bdat;

        match bdat {
            Bdat::Unknown => Value::Null,
            Bdat::UnsignedByte(n) | Bdat::Unknown2(n) => n.into(),
            Bdat::UnsignedShort(n) | Bdat::Unknown3(n) => n.into(),
            Bdat::UnsignedInt(n) | Bdat::Unknown1(n) => n.into(),
            Bdat::SignedByte(n) => n.into(),
            Bdat::SignedShort(n) => n.into(),
            Bdat::SignedInt(n) => n.into(),
            Bdat::String(s) => Value::String(s),
            Bdat::Float(f) => f.into(),
            Bdat::HashRef(n) => Value::String(Label::Hash(n).to_string()),
            Bdat::Percent(f) => (f as f32 * 0.01).into(),
        }
    }
}

impl BdatSerialize for JsonConverter {
    fn write_table(&self, table: RawTable, writer: &mut dyn Write) -> Result<()> {
        let schema = (!self.untyped).then(|| {
            table
                .columns
                .iter()
                .map(|c| {
                    json! ({
                        "name": c.label.to_string(),
                        "type": c.ty as u8,
                        "hashed": matches!(c.label, Label::Unhashed(_)),
                    })
                })
                .collect::<Vec<_>>()
        });

        let rows = table
            .rows
            .into_iter()
            .map(|mut row| {
                let mut doc = Map::default();
                doc.insert(String::from("$id"), row.id.into());
                for col in &table.columns {
                    doc.insert(
                        col.label.to_string(),
                        match row.cells.remove(0) {
                            Cell::Single(v) => self.convert(v),
                            Cell::Flag(f) => f.into(),
                            Cell::List(l) => l
                                .into_iter()
                                .map(|v| self.convert(v))
                                .collect::<Vec<_>>()
                                .into(),
                        },
                    );
                }
                Value::Object(doc)
            })
            .collect::<Vec<_>>();

        let json = json!({
            "schema": schema,
            "rows": rows
        });
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
