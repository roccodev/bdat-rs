use anyhow::{Context, Result};
use bdat::serde::SerializeCell;
use bdat::{types::Table, Cell, ColumnDef, Value};
use clap::Args;
use csv::WriterBuilder;
use std::io::Write;
use std::iter::Once;

use super::{BdatSerialize, ConvertArgs};

#[derive(Args)]
pub struct CsvOptions {
    #[arg(long)]
    csv_separator: Option<char>,
    /// When converting to CSV, expands legacy-BDAT lists into separate columns
    #[arg(long)]
    expand_lists: bool,
}

pub struct CsvConverter {
    separator_ch: char,
    expand_lists: bool,
}

/// Utility to `flat_map` multiple iterator types
enum ColumnIter<E, T: Iterator<Item = E>, T2: Iterator<Item = E>> {
    Single(Once<E>),
    Flags(T),
    Array(T2),
}

impl CsvConverter {
    pub fn new(args: &ConvertArgs) -> Self {
        Self {
            separator_ch: args.csv_opts.csv_separator.unwrap_or(','),
            expand_lists: args.csv_opts.expand_lists,
        }
    }

    fn format_column<'a>(
        &self,
        column: &'a ColumnDef,
    ) -> ColumnIter<String, impl Iterator<Item = String> + 'a, impl Iterator<Item = String> + 'a>
    {
        if !column.flags().is_empty() {
            return ColumnIter::Flags(
                column
                    .flags()
                    .iter()
                    .map(|flag| format!("{} [{}]", column.label(), flag.label())),
            );
        }
        if column.count() > 1 && self.expand_lists {
            return ColumnIter::Array(
                (0..column.count()).map(|i| format!("{}[{i}]", column.label())),
            );
        }
        ColumnIter::Single(std::iter::once(column.label().to_string()))
    }

    fn format_cell<'b, 'a: 'b, 't: 'a>(
        &self,
        column: &'a ColumnDef,
        cell: &'b Cell<'t>,
    ) -> ColumnIter<
        SerializeCell<'a, 'b, 't>,
        impl Iterator<Item = SerializeCell<'a, 'b, 't>>,
        impl Iterator<Item = SerializeCell<'a, 'b, 't>>,
    > {
        match cell {
            // Single values: serialize normally
            c @ Cell::Single(_) => ColumnIter::Single(std::iter::once(column.cell_serializer(c))),
            // List values + expand lists: serialize into multiple columns
            Cell::List(values) if self.expand_lists => ColumnIter::Array(
                values
                    .iter()
                    .map(|v| column.owned_cell_serializer(Cell::Single(v.clone()))),
            ),
            // List values: serialize as JSON
            Cell::List(values) => {
                ColumnIter::Single(std::iter::once(column.owned_cell_serializer(Cell::Single(
                    Value::String(serde_json::to_string(values).unwrap().into()),
                ))))
            }
            // Flags: serialize into multiple columns
            Cell::Flags(flags) => ColumnIter::Flags(
                flags
                    .iter()
                    .map(|i| column.owned_cell_serializer(Cell::Single(Value::UnsignedInt(*i)))),
            ),
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
            .flat_map(|c| self.format_column(c))
            .collect::<Vec<_>>();

        writer.serialize(header).context("Failed to write header")?;

        for row in table.rows() {
            let serialized_row = row
                .cells()
                .zip(table.columns())
                .flat_map(|(cell, col)| self.format_cell(col, cell))
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

impl<E, T: Iterator<Item = E>, T2: Iterator<Item = E>> Iterator for ColumnIter<E, T, T2> {
    type Item = E;

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            Self::Single(i) => i.next(),
            Self::Flags(i) => i.next(),
            Self::Array(i) => i.next(),
        }
    }
}
