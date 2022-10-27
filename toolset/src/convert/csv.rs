use anyhow::{Context, Result};
use bdat::types::{Cell, RawTable};
use clap::Args;
use std::io::Write;

use super::BdatSerialize;

#[derive(Args)]
pub struct CsvOptions {
    #[arg(long)]
    csv_separator: Option<char>,
}

pub struct CsvConverter {
    separator: String,
    separator_ch: char,
}

impl CsvConverter {
    pub fn new(opts: CsvOptions) -> Self {
        let separator_ch = opts.csv_separator.unwrap_or(',');
        Self {
            separator_ch,
            separator: separator_ch.to_string(),
        }
    }

    fn escape(&self, val: String) -> String {
        // If a field contains a field separator, new line, or double quotes, the field
        // is escaped by wrapping it in double quotes.
        if val
            .chars()
            .any(|c| c == self.separator_ch || c == '"' || c == '\n')
        {
            let mut escaped = String::with_capacity(val.len() * 2);
            escaped.push('"');
            for c in val.chars() {
                if c == '"' {
                    // Additionally, any double quotes inside the field should be
                    // escaped with extra double quotes.
                    escaped.push('"');
                }
                escaped.push(c);
            }
            escaped.push('"');
            escaped
        } else {
            val
        }
    }
}

impl BdatSerialize for CsvConverter {
    fn write_table(&self, table: RawTable, writer: &mut dyn Write) -> Result<()> {
        let header = table
            .columns()
            .map(|c| format!("{}@{}", c.ty as u8, c.label))
            .collect::<Vec<_>>()
            .join(&self.separator);
        writeln!(writer, "{}", header).context("Failed to write header")?;
        for row in table.rows() {
            let formatted = row
                .cells()
                .map(|c| match c {
                    Cell::Single(v) => self.escape(v.to_string()),
                    Cell::Flag(f) => self.escape(f.to_string()),
                    Cell::List(_) => todo!(),
                })
                .collect::<Vec<_>>()
                .join(&self.separator);
            writeln!(writer, "{}", formatted)
                .with_context(|| format!("Failed to write row {}", row.id()))?;
        }
        Ok(())
    }

    fn get_file_name(&self, table_name: &str) -> String {
        format!("{table_name}.csv")
    }
}
