use std::{
    ffi::OsStr,
    fs::File,
    io::{BufReader, BufWriter, Read, Write},
    path::Path,
};

use anyhow::{Context, Result};
use bdat::{CompatTable, Label};
use clap::Args;
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use rayon::prelude::*;

use crate::{
    error::Error,
    filter::{Filter, FilterArg},
    util::hash::HashNameTable,
    InputData,
};
use crate::{
    error::SchemaError,
    util::{BdatGame, ProgressBarState, RayonPoolJobs},
};

use self::schema::{AsFileName, FileSchema};

mod csv;
mod json;
mod schema;

#[derive(Args)]
pub struct ConvertArgs {
    /// The output directory that should contain the conversion result.
    #[arg(short, long)]
    out_dir: Option<String>,
    /// Specifies the file type for the output file (when extracting) and input files (when packing).
    #[arg(short, long)]
    file_type: Option<String>,
    /// (Extract only) If this is set, types are not included in the serialized files. Note: the extracted output
    /// cannot be repacked without type information
    #[arg(short, long)]
    untyped: bool,
    /// (Extract only) If this is set, a schema file is not generated. Note: the extracted output cannot be
    /// repacked without a schema
    #[arg(short = 's', long)]
    no_schema: bool,
    /// Only convert these tables. If absent, converts all tables from all files.
    #[arg(short, long)]
    tables: Vec<String>,

    #[clap(flatten)]
    jobs: RayonPoolJobs,

    #[clap(flatten)]
    csv_opts: csv::CsvOptions,
    #[clap(flatten)]
    json_opts: json::JsonOptions,
}

pub trait BdatSerialize {
    /// Writes a converted BDAT table to a [`Write`] implementation.
    fn write_table(&self, table: CompatTable, writer: &mut dyn Write) -> Result<()>;

    /// Formats the file name for a converted BDAT table.
    fn get_file_name(&self, table_name: &str) -> String;
}

pub trait BdatDeserialize {
    /// Reads a BDAT table from a file.
    fn read_table(
        &self,
        name: Label<'static>,
        schema: &FileSchema,
        reader: &mut dyn Read,
    ) -> Result<CompatTable>;

    /// Returns the file extension used in converted table files
    fn get_table_extension(&self) -> &'static str;
}

pub fn run_conversions(input: InputData, args: ConvertArgs, is_extracting: bool) -> Result<()> {
    args.jobs.configure()?;

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
        .out_dir
        .as_ref()
        .ok_or(Error::MissingRequiredArgument("out-dir"))?;
    let out_dir = Path::new(&out_dir);
    std::fs::create_dir_all(out_dir).context("Could not create output directory")?;

    let serializer: Box<dyn BdatSerialize + Send + Sync> = match args
        .file_type
        .as_ref()
        .ok_or(Error::MissingRequiredArgument("file-type"))?
        .as_str()
    {
        "csv" => Box::new(csv::CsvConverter::new(&args)),
        "json" => Box::new(json::JsonConverter::new(&args)),
        t => return Err(Error::UnknownFileType(t.to_string()).into()),
    };

    let table_filter: Filter = args.tables.into_iter().map(FilterArg).collect();

    let files = input
        .list_files("bdat", false)?
        .into_iter()
        .collect::<walkdir::Result<Vec<_>>>()?;
    let base_path = crate::util::get_common_denominator(&files);

    let multi_bar = MultiProgress::new();
    let file_bar = multi_bar
        .add(ProgressBar::new(files.len() as u64).with_style(build_progress_style("Files", true)));
    let table_bar_style = build_progress_style("Tables", false);

    let res = files
        .into_par_iter()
        .panic_fuse()
        .map(|path| {
            let mut file = std::fs::read(&path)?;
            let game = input.game_from_bytes(&file)?;
            let tables = game.from_bytes(&mut file).with_context(|| {
                format!("Could not parse BDAT tables ({})", path.to_string_lossy())
            })?;

            let file_name = path
                .file_stem()
                .and_then(OsStr::to_str)
                .map(ToString::to_string)
                .unwrap();

            file_bar.inc(0);
            let table_bar = multi_bar
                .add(ProgressBar::new(tables.len() as u64).with_style(table_bar_style.clone()));

            let out_dir = out_dir.join(
                path.strip_prefix(&base_path)
                    .unwrap()
                    .parent()
                    .unwrap_or_else(|| Path::new("")),
            );
            let tables_dir = out_dir.join(&file_name);
            std::fs::create_dir_all(&tables_dir)?;

            let mut schema = (!args.no_schema).then(|| FileSchema::new(file_name, game.into()));

            for mut table in tables {
                hash_table.convert_all(&mut table);

                if let Some(schema) = &mut schema {
                    schema.feed_table(&table);
                }

                let name = table.name();
                if !table_filter.contains(&name) {
                    continue;
                }

                // {:+} displays hashed names without brackets (<>)
                let out_file =
                    File::create(tables_dir.join(serializer.get_file_name(&name.as_file_name())))
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
    let schema_files = input
        .list_files("bschema", false)?
        .into_iter()
        .collect::<walkdir::Result<Vec<_>>>()?;
    if schema_files.is_empty() {
        return Err(Error::from(SchemaError::MissingSchema).into());
    }
    let base_path = crate::util::get_common_denominator(&schema_files);

    let out_dir = args
        .out_dir
        .as_ref()
        .ok_or(Error::MissingRequiredArgument("out-dir"))?;
    let out_dir = Path::new(&out_dir);
    std::fs::create_dir_all(out_dir).context("Could not create output directory")?;

    let deserializer: Box<dyn BdatDeserialize + Send + Sync> = match args
        .file_type
        .as_ref()
        .ok_or(Error::MissingRequiredArgument("file-type"))?
        .as_str()
    {
        "json" => Box::new(json::JsonConverter::new(&args)),
        t => return Err(Error::UnknownFileType(t.to_string()).into()),
    };

    let progress_bar = ProgressBarState::new("Files", "Tables", schema_files.len());

    progress_bar.master_bar.inc(0);
    let res = schema_files
        .into_par_iter()
        .panic_fuse()
        .map(|schema_path| {
            let schema_file = FileSchema::read(File::open(&schema_path)?)?;

            // The relative path to the tables (we mimic the original file structure in the output)
            let relative_path = schema_path
                .strip_prefix(&base_path)
                .unwrap()
                .parent()
                .unwrap_or_else(|| Path::new(""));

            let table_bar = progress_bar.add_child(schema_file.table_count());

            // Tables are stored at <relative root>/<file name>
            let tables = schema_file
                .find_table_files(
                    &schema_path.parent().unwrap().join(&schema_file.file_name),
                    deserializer.get_table_extension(),
                )
                .into_par_iter()
                .panic_fuse()
                .map(|(label, table)| {
                    let table_file = File::open(table)?;
                    let mut reader = BufReader::new(table_file);

                    table_bar.inc(1);
                    deserializer.read_table(
                        label.into_hash(schema_file.version).into_owned(),
                        &schema_file,
                        &mut reader,
                    )
                })
                .collect::<Result<Vec<_>>>()?;

            if tables.is_empty() {
                progress_bar.println(format!(
                    "[Warn] File {} has no tables",
                    schema_path.display()
                ))?;
            }

            progress_bar.remove_child(&table_bar);

            let out_dir = out_dir.join(relative_path);
            std::fs::create_dir_all(&out_dir)?;
            let out_file = File::create(out_dir.join(format!("{}.bdat", schema_file.file_name)))?;
            let game = input
                .game
                .unwrap_or_else(|| BdatGame::version_default(schema_file.version));
            game.to_writer(out_file, tables)?;
            progress_bar.master_bar.inc(1);
            Ok(())
        })
        .find_any(|r: &anyhow::Result<()>| r.is_err());

    if let Some(r) = res {
        r?;
    }

    progress_bar.finish();
    Ok(())
}

pub fn build_progress_style(label: &str, with_time: bool) -> ProgressStyle {
    ProgressStyle::with_template(&match with_time {
        true => format!("{{spinner:.cyan}} [{{elapsed_precise:.cyan}}] {label}{{msg}}: {{human_pos}}/{{human_len}} ({{percent}}%) [{{bar:.cyan/blue}}] ETA: {{eta}}"),
        false => format!("{{spinner:.green}} {label}{{msg}}: {{human_pos}}/{{human_len}} ({{percent}}%) [{{bar}}]"),
    })
    .unwrap()
}
