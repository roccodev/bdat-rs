use crate::{
    filter::{Filter, FilterArg},
    hash::HashNameTable,
    InputData,
};
use anyhow::{Context, Result};
use bdat::types::Label;
use clap::Args;
use std::borrow::Cow;

#[derive(Args)]
pub struct InfoArgs {
    /// Only check these tables. If absent, returns data from all tables.
    #[arg(short, long)]
    tables: Vec<String>,
    /// Only print these columns. If absent, prints all columns.
    #[arg(short, long)]
    columns: Vec<String>,
}

pub fn get_info(input: InputData, args: InfoArgs) -> Result<()> {
    let hash_table = input.load_hashes()?;
    let table_filter: Filter = args.tables.into_iter().map(FilterArg).collect();
    let column_filter: Filter = args.columns.into_iter().map(FilterArg).collect();

    for file in input.list_files("bdat", false)? {
        let path = file?;
        let mut file = std::fs::read(&path)?;
        let tables = input
            .game_from_bytes(&file)?
            .from_bytes(&mut file)
            .with_context(|| format!("Could not parse BDAT tables ({})", path.to_string_lossy()))?;
        for table in tables {
            let name = match table.name() {
                Some(n) => {
                    if !table_filter.contains(n) {
                        continue;
                    }
                    n
                }
                None => continue,
            };
            println!("Table {}", format_unhashed_label(name, &hash_table));
            println!(
                "  Columns: {} / Rows: {}",
                table.column_count(),
                table.row_count()
            );

            if table.column_count() != 0 {
                println!("  Columns:");
                let mut offset = 0;
                for col in table
                    .into_columns()
                    .filter(|c| column_filter.contains(c.label()))
                {
                    let mut extra = Cow::Borrowed("");
                    if col.count() > 1 {
                        extra = Cow::Owned(format!("[{}]", col.count()));
                    }

                    println!(
                        "    - [{}] {}: {:?}{}",
                        offset,
                        format_unhashed_label(col.label(), &hash_table),
                        col.value_type(),
                        extra
                    );

                    for flag in col.flags() {
                        println!(
                            "      + [(v & 0x{:X}) >> {}] {}: Flag",
                            flag.mask(),
                            flag.shift_amount(),
                            flag.label()
                        );
                    }

                    offset += col.data_size();
                }
            }
        }
    }

    Ok(())
}

fn format_unhashed_label(label: &Label, hash_table: &HashNameTable) -> String {
    let previous_hash = match label {
        Label::Hash(h) => Some(*h),
        _ => None,
    };

    match (hash_table.convert_label_cow(label).as_ref(), previous_hash) {
        (l @ Label::Unhashed(_), Some(hash)) => format!("{l} (<{hash:08X}>)"),
        (l, _) => l.to_string(),
    }
}
