use std::fs::File;

use crate::InputData;
use anyhow::{Context, Result};
use bdat::read::{BdatFile, LittleEndian};

pub fn get_info(input: InputData) -> Result<()> {
    let file = File::open(input.in_file)?;
    let mut file = BdatFile::<_, LittleEndian>::read(file).context("Failed to read BDAT file")?;

    for table in file.get_tables().context("Could not parse BDAT tables")? {
        println!("Table {}", table.name.expect("todo"));
        println!(
            "  Columns: {} / Rows: {}",
            table.columns.len(),
            table.rows.len()
        );
        if !table.columns.is_empty() {
            println!("  Columns:");
            for col in table.columns {
                println!("    - [{}] {}: {:?}", col.offset, col.label, col.ty);
            }
        }
    }

    Ok(())
}
