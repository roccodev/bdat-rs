use std::{
    fs::File,
    io::{BufReader, BufWriter, Write},
    path::Path,
};

use anyhow::{Context, Result};
use bdat::{
    read::{BdatFile, LittleEndian},
    types::RawTable,
};
use clap::{error::ErrorKind, Args, Error};
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};

use crate::{
    filter::{Filter, FilterArg},
    InputData,
};

mod csv;

#[derive(Args)]
pub struct ConvertArgs {
    /// When deserializing, this is the BDAT file to be generated. When serializing, this is the output file or directory that
    /// should contain the serialization result.
    #[arg(short, long)]
    out_file: Option<String>,
    /// Forces a specific file type for the output file (when serializing) and input files (when deserializing).
    /// When deserializing, the file type will be detected automatically if this is missing.
    #[arg(short, long)]
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
    fn write_table(&mut self, table: RawTable, writer: &mut dyn Write) -> Result<()>;

    fn get_file_name(&self, table_name: &str) -> String;
}

pub trait BdatDeserialize {
    fn read_table(&self) -> Result<RawTable>;
}

pub fn run_conversions(input: InputData, args: ConvertArgs) -> Result<()> {
    run_serialization(input, args)
}

pub fn run_serialization(input: InputData, args: ConvertArgs) -> Result<()> {
    let out_dir = args
        .out_file
        .ok_or_else(|| Error::raw(ErrorKind::MissingRequiredArgument, "out dir is required"))?;
    let out_dir = Path::new(&out_dir);
    std::fs::create_dir(out_dir).context("Could not create output directory")?;

    let mut serializer: Box<dyn BdatSerialize> = match args
        .file_type
        .ok_or_else(|| Error::raw(ErrorKind::MissingRequiredArgument, "file type is required"))?
        .as_str()
    {
        "csv" => Box::new(csv::CsvConverter::new(args.csv_opts)),
        t => {
            return Err(Error::raw(
                ErrorKind::ValueValidation,
                format!("unknown file type '{}'", t),
            )
            .into())
        }
    };

    let table_filter: Filter = args.tables.into_iter().map(FilterArg).collect();
    let column_filter: Filter = args.columns.into_iter().map(FilterArg).collect();

    let files = input.list_files("bdat").into_iter().collect::<Vec<_>>();

    let multi_bar = MultiProgress::new();
    let file_bar = multi_bar.add(
        ProgressBar::new(files.len() as u64).with_style(
            ProgressStyle::with_template(
                "{spinner:.green} Files: {human_pos}/{human_len} ({percent}%) [{bar}] ETA: {eta}",
            )
            .unwrap(),
        ),
    );
    let table_bar = multi_bar.add(
        ProgressBar::new(0).with_style(
            ProgressStyle::with_template(
                "{spinner:.green} Tables: {human_pos}/{human_len} ({percent}%) [{bar}]",
            )
            .unwrap(),
        ),
    );

    for file in files {
        let path = file?;
        let file = BufReader::new(File::open(&path)?);
        let mut file =
            BdatFile::<_, LittleEndian>::read(file).context("Failed to read BDAT file")?;

        file_bar.inc(0);
        table_bar.reset();
        table_bar.set_length(file.header.table_count as u64);

        for table in file
            .get_tables()
            .with_context(|| format!("Could not parse BDAT tables ({})", path.to_string_lossy()))?
        {
            let name = match table.name {
                Some(ref n) => {
                    if !table_filter.contains(&n) {
                        continue;
                    }
                    n
                }
                None => {
                    eprintln!(
                        "[Warn] Found unnamed table in {}",
                        path.file_name().unwrap().to_string_lossy()
                    );
                    continue;
                }
            };

            // {:+} displays hashed names without brackets (<>)
            let out_file =
                File::create(out_dir.join(serializer.get_file_name(&format!("{:+}", name))))
                    .context("Could not create output file")?;
            let mut writer = BufWriter::new(out_file);
            serializer
                .write_table(table, &mut writer)
                .context("Could not write table")?;
            writer.flush().context("Could not save table")?;

            table_bar.inc(1);
        }

        file_bar.inc(1);
    }

    table_bar.finish();
    file_bar.finish();

    Ok(())
}

fn serialize_tables<T, E>(in_file: BdatFile<T, E>, out_dir: &str) {}
