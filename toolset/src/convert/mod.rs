use anyhow::Result;
use bdat::{read::BdatFile, types::RawTable};
use clap::Args;

use crate::InputData;

mod csv;

#[derive(Args)]
pub struct ConvertArgs {
    /// When deserializing, this is the BDAT file to be generated. When serializing, this is the output file or directory that
    /// should contain the serialization result.
    #[arg(short, long)]
    out_file: Option<String>,
    /// Forces a specific file type for the output file (when serializing) and input files (when deserializing).
    /// When deserializing, the file type will be detected automatically if this is missing.
    #[arg(short = 't', long)]
    file_type: Option<String>,
    /// (Serialization only) If this is set, types are not included in the serialized files. Instead, they will be placed
    /// inside the schema file.
    #[arg(short, long)]
    untyped: bool,
    /// (Serialization only) If this is set, a schema file is not generated. Note: the serialized output cannot be
    /// deserialized without a schema
    #[arg(short = 's', long)]
    no_schema: bool,
    /// Only convert these tables. If absent, converts all tables from all files.
    #[arg(short, long)]
    tables: Vec<String>,
    /// Only convert these columns. If absent, converts all columns.
    #[arg(short, long)]
    columns: Vec<String>,

    #[clap(flatten)]
    csv_opts: csv::CsvOptions,
}

pub trait BdatSerialize {
    fn write_table(&mut self, table: RawTable) -> Result<()>;
}

pub trait BdatDeserialize {
    fn read_table(&self) -> Result<RawTable>;
}

pub fn run_conversions(input: InputData, args: ConvertArgs) -> Result<()> {
    Ok(())
}

fn serialize_tables<T, E>(in_file: BdatFile<T, E>, out_dir: &str) {}
