use anyhow::{Context, Result};
use bdat::types::{Cell, RawTable};
use clap::Args;
use std::io::Write;

use super::BdatSerialize;

#[derive(Args)]
pub struct CsvOptions {
    #[arg(long)]
    csv_separator: Option<String>,
}

struct CsvConverter<W> {
    writer: W,
    separator: &'static str,
}

impl<W> BdatSerialize for CsvConverter<W>
where
    W: Write,
{
    fn write_table(&mut self, table: RawTable) -> Result<()> {
        let header = table
            .columns
            .iter()
            .map(|c| format!("{}@{}", c.ty as u8, c.label))
            .collect::<Vec<_>>()
            .join(self.separator);
        writeln!(self.writer, "{}", header).context("Failed to write header")?;
        for row in table.rows {
            let formatted = row
                .cells
                .iter()
                .map(|c| match c {
                    Cell::Single(v) => v.to_string(),
                    Cell::Flag(f) => f.to_string(),
                    Cell::List(_) => todo!(),
                })
                .collect::<Vec<_>>()
                .join(self.separator);
            writeln!(self.writer, "{}", formatted)
                .with_context(|| format!("Failed to write row {}", row.id))?;
        }
        Ok(())
    }
}
