use std::{
    ffi::OsStr,
    fs::File,
    io::{BufReader, BufWriter, Read, Write},
    path::Path,
};

use anyhow::{Context, Result};
use bdat::{
    io::{BdatFile, BdatVersion, LittleEndian, SwitchBdatFile},
    types::{Label, RawTable},
};
use clap::{error::ErrorKind, Args, Error};
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use rayon::prelude::*;

use crate::{
    filter::{Filter, FilterArg},
    hash::HashNameTable,
    InputData,
};

use self::schema::{AsFileName, FileSchema};

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
    /// Reads a BDAT table from a file.
    fn read_table(
        &self,
        name: Option<Label>,
        schema: &FileSchema,
        reader: &mut dyn Read,
    ) -> Result<RawTable>;
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

    if is_extracting {
        let hash_table = input.load_hashes()?;
        run_serialization(input, args, hash_table)
    } else {
        run_deserialization(input, args)
    }
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

            let mut schema = (!args.no_schema).then(|| {
                FileSchema::new(
                    path.file_stem()
                        .and_then(OsStr::to_str)
                        .map(ToString::to_string)
                        .unwrap(),
                    args.untyped,
                )
            });

            for mut table in file.get_tables().with_context(|| {
                format!("Could not parse BDAT tables ({})", path.to_string_lossy())
            })? {
                hash_table.convert_all(&mut table);

                if let Some(schema) = &mut schema {
                    schema.feed_table(&table);
                }

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

            if let Some(schema) = schema {
                schema.write(out_dir)?;
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
    let schema_files = input.list_files("bschema").into_iter().collect::<Vec<_>>();
    if schema_files.is_empty() {
        todo!("no schema files found; schema files are required for deserialization");
    }

    let deserializer: Box<dyn BdatDeserialize + Send + Sync> = match args
        .file_type
        .as_ref()
        .ok_or_else(|| Error::raw(ErrorKind::MissingRequiredArgument, "file type is required"))? // TODO
        .as_str()
    {
        "json" => Box::new(json::JsonConverter::new(&args)),
        t => {
            return Err(Error::raw(
                ErrorKind::ValueValidation,
                format!("unknown file type '{}'", t),
            )
            .into())
        }
    };

    let multi_bar = MultiProgress::new();
    let file_bar = multi_bar.add(
        ProgressBar::new(schema_files.len() as u64).with_style(
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

    file_bar.inc(0);
    schema_files
        .into_par_iter()
        .map(|schema_path| {
            let schema_path = schema_path?;
            let schema_file = FileSchema::read(File::open(&schema_path)?)?;
            let table_bar = multi_bar.add(
                ProgressBar::new(schema_file.table_count() as u64)
                    .with_style(table_bar_style.clone()),
            );

            let tables = schema_file
                .find_table_files(schema_path.parent().unwrap(), "json")
                .into_par_iter()
                .map(|table| {
                    let table_file = File::open(&table)?;
                    let mut reader = BufReader::new(table_file);

                    table_bar.inc(1);
                    // TODO check force hash
                    Ok(deserializer.read_table(
                        Some(Label::parse(
                            table.file_stem().unwrap().to_string_lossy().to_string(),
                            true,
                        )),
                        &schema_file,
                        &mut reader,
                    )?)
                })
                .collect::<Vec<Result<_>>>();

            multi_bar.remove(&table_bar);

            let out_file =
                File::create(Path::new("/tmp/").join(&format!("{}.bdat", schema_file.file_name)))?;
            let mut out_file = SwitchBdatFile::new(out_file, BdatVersion::Modern);
            out_file.write_all_tables(tables.into_iter().flatten().collect::<Vec<_>>())?; // TODO handle
                                                                                          // errors

            file_bar.inc(1);

            Ok(())
        })
        .find_any(|r: &anyhow::Result<()>| r.is_err());

    file_bar.finish();
    Ok(())
}
