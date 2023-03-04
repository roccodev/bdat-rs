use anyhow::{Context, Result};
use bdat::{types::Table, ColumnDef};
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
    untyped: bool,
}

impl CsvConverter {
    pub fn new(args: &ConvertArgs) -> Self {
        Self {
            separator_ch: args.csv_opts.csv_separator.unwrap_or(','),
            untyped: args.untyped,
        }
    }

    fn format_column(&self, column: &ColumnDef) -> String {
        if self.untyped {
            column.label().to_string()
        } else {
            format!("{}@{}", column.value_type() as u8, column.label())
        }
    }
}

impl BdatSerialize for CsvConverter {
    fn write_table(&self, table: Table, writer: &mut dyn Write) -> Result<()> {
        let mut writer = WriterBuilder::new()
            .delimiter(self.separator_ch as u8)
            .from_writer(writer);
        let header = table
            .columns()
            .map(|c| self.format_column(c))
            .collect::<Vec<_>>();
        writer.serialize(header).context("Failed to write header")?;
        for row in table.rows() {
            writer
                .serialize(row.cells().collect::<Vec<_>>())
                .with_context(|| format!("Failed to write row {}", row.id()))?;
        }
        Ok(())
    }

    fn get_file_name(&self, table_name: &str) -> String {
        format!("{table_name}.csv")
    }
}
