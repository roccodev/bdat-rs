use std::fs::File;

use crate::{
    filter::{Filter, FilterArg},
    InputData,
};
use anyhow::{Context, Result};
use bdat::read::{BdatFile, LittleEndian};
use clap::Args;

#[derive(Args)]
pub struct InfoArgs {
    /// Only check these tables. If absent, returns data from all tables.
    #[arg(short, long)]
    tables: Vec<String>,
    /// Only print these columns. If absent, prints all columns.
    #[arg(short, long)]
    columns: Vec<String>,
    /// If this is set, saves a copy of the file's schema in the specified directory.
    #[arg(short, long)]
    out_schema: Option<String>,
}

pub fn get_info(input: InputData, args: InfoArgs) -> Result<()> {
    let file = File::open(input.in_file)?;
    let mut file = BdatFile::<_, LittleEndian>::read(file).context("Failed to read BDAT file")?;

    let table_filter: Filter = args.tables.into_iter().map(FilterArg).collect();
    let column_filter: Filter = args.columns.into_iter().map(FilterArg).collect();

    for table in file.get_tables().context("Could not parse BDAT tables")? {
        let name = match table.name {
            Some(n) => {
                if !table_filter.contains(&n) {
                    continue;
                }
                n
            }
            None => continue,
        };
        println!("Table {}", name);
        println!(
            "  Columns: {} / Rows: {}",
            table.columns.len(),
            table.rows.len()
        );
        if !table.columns.is_empty() {
            println!("  Columns:");
            for col in table
                .columns
                .into_iter()
                .filter(|c| column_filter.contains(&c.label))
            {
                println!("    - [{}] {}: {:?}", col.offset, col.label, col.ty);
            }
        }
    }

    Ok(())
}
