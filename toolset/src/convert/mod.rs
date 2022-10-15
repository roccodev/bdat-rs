use std::{
    fs::File,
    io::{BufReader, BufWriter, Write},
    path::Path,
};

use anyhow::{Context, Result};
use bdat::{
    io::{BdatFile, LittleEndian},
    types::RawTable,
};
use clap::{error::ErrorKind, Args, Error};
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use rayon::prelude::*;

use crate::{
    filter::{Filter, FilterArg},
    hash::HashNameTable,
    InputData,
};

use self::schema::AsFileName;

mod csv;
mod json;
mod schema;

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
    /// The number of jobs (or threads) to use in the conversion process.
    /// By default, this is the number of cores/threads in the system.
    #[arg(short, long)]
    jobs: Option<u16>,

    #[clap(flatten)]
    csv_opts: csv::CsvOptions,
    #[clap(flatten)]
    json_opts: json::JsonOptions,
}

pub trait BdatSerialize {
    fn write_table(&self, table: RawTable, writer: &mut dyn Write) -> Result<()>;

    fn get_file_name(&self, table_name: &str) -> String;
}

pub trait BdatDeserialize {
    fn read_table(&self) -> Result<RawTable>;
}

pub fn run_conversions(input: InputData, args: ConvertArgs, is_extracting: bool) -> Result<()> {
    // Change number of jobs in Rayon's thread pool
    let mut pool_builder = rayon::ThreadPoolBuilder::new();
    if let Some(jobs) = args.jobs {
        pool_builder = pool_builder.num_threads(jobs as usize);
    }
    pool_builder
        .build_global()
        .context("Could not build thread pool")?;

    let hash_table = input.load_hashes()?;

    run_serialization(input, args, hash_table)
}

pub fn run_serialization(
    input: InputData,
    args: ConvertArgs,
    hash_table: HashNameTable,
) -> Result<()> {
    let out_dir = args
        .out_file
        .as_ref()
        .ok_or_else(|| Error::raw(ErrorKind::MissingRequiredArgument, "out dir is required"))?;
    let out_dir = Path::new(&out_dir);
    std::fs::create_dir(out_dir).context("Could not create output directory")?;

    let serializer: Box<dyn BdatSerialize + Send + Sync> = match args
        .file_type
        .as_ref()
        .ok_or_else(|| Error::raw(ErrorKind::MissingRequiredArgument, "file type is required"))?
        .as_str()
    {
        "csv" => Box::new(csv::CsvConverter::new(args.csv_opts)),
        "json" => Box::new(json::JsonConverter::new(&args)),
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

    let table_bar_style = ProgressStyle::with_template(
        "{spinner:.green} Tables: {human_pos}/{human_len} ({percent}%) [{bar}]",
    )
    .unwrap();

    let res = files
        .into_par_iter()
        .map(|file| {
            let path = file?;
            let file = BufReader::new(File::open(&path)?);
            let mut file =
                BdatFile::<_, LittleEndian>::read(file).context("Failed to read BDAT file")?;

            file_bar.inc(0);
            let table_bar = multi_bar.add(
                ProgressBar::new(file.table_count() as u64).with_style(table_bar_style.clone()),
            );

            for mut table in file.get_tables().with_context(|| {
                format!("Could not parse BDAT tables ({})", path.to_string_lossy())
            })? {
                hash_table.convert_all(&mut table);

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
                    File::create(out_dir.join(serializer.get_file_name(&name.as_file_name())))
                        .context("Could not create output file")?;
                let mut writer = BufWriter::new(out_file);
                serializer
                    .write_table(table, &mut writer)
                    .context("Could not write table")?;
                writer.flush().context("Could not save table")?;

                table_bar.inc(1);
            }

            file_bar.inc(1);
            multi_bar.remove(&table_bar);

            Ok(())
        })
        .find_any(|r: &anyhow::Result<()>| r.is_err());

    if let Some(r) = res {
        r?;
    }

    file_bar.finish();

    Ok(())
}

fn run_deserialization(input: InputData, args: ConvertArgs) -> Result<()> {
    Ok(())
}
