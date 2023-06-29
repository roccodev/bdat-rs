use anyhow::{Context, Result};
use bdat::{types::Table, Cell, ColumnDef, FlagDef, Value};
use clap::Args;
use csv::WriterBuilder;
use std::io::Write;

use super::{BdatSerialize, ConvertArgs};

#[derive(Args)]
pub struct CsvOptions {
    #[arg(long)]
    csv_separator: Option<char>,
}

pub struct CsvConverter {
    separator_ch: char,
}

impl CsvConverter {
    pub fn new(args: &ConvertArgs) -> Self {
        Self {
            separator_ch: args.csv_opts.csv_separator.unwrap_or(','),
        }
    }

    fn format_column(&self, column: &ColumnDef) -> String {
        column.label().to_string()
    }

    fn format_flag(&self, flag: &FlagDef, parent: &ColumnDef) -> String {
        format!("{} [{}]", parent.label(), flag.label())
    }
}

impl BdatSerialize for CsvConverter {
    fn write_table(&self, table: Table, writer: &mut dyn Write) -> Result<()> {
        let mut writer = WriterBuilder::new()
            .delimiter(self.separator_ch as u8)
            .from_writer(writer);

        let header = table
            .columns()
            .flat_map(|c| {
                if c.flags().is_empty() {
                    vec![self.format_column(c)].into_iter()
                } else {
                    c.flags()
                        .iter()
                        .map(|f| self.format_flag(f, c))
                        .collect::<Vec<_>>()
                        .into_iter()
                }
            })
            .collect::<Vec<_>>();

        writer.serialize(header).context("Failed to write header")?;

        for row in table.rows() {
            let serialized_row = row
                .cells()
                .zip(table.columns())
                .flat_map(|(cell, col)| match cell {
                    // Flags: serialize as multiple integers
                    Cell::Flags(flags) => flags
                        .iter()
                        .map(|i| col.owned_cell_serializer(Cell::Single(Value::UnsignedInt(*i))))
                        .collect::<Vec<_>>()
                        .into_iter(),
                    // Array: serialize as JSON
                    Cell::List(list) => vec![col.owned_cell_serializer(Cell::Single(
                        Value::String(serde_json::to_string(list).unwrap().into()),
                    ))]
                    .into_iter(),
                    // Single: serialize normally
                    _ => vec![col.cell_serializer(cell)].into_iter(),
                })
                .collect::<Vec<_>>();
            writer
                .serialize(serialized_row)
                .with_context(|| format!("Failed to write row {}", row.id()))?;
        }
        Ok(())
    }

    fn get_file_name(&self, table_name: &str) -> String {
        format!("{table_name}.csv")
    }
}
