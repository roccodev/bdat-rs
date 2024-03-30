use crate::error::Error;
use crate::util::{ProgressBarState, RayonPoolJobs};
use crate::InputData;
use anyhow::{Context, Result};
use bdat::legacy::scramble::ScrambleType;
use bdat::legacy::{FileHeader, TableHeader};
use bdat::{BdatVersion, LegacyVersion, SwitchEndian, WiiEndian};
use clap::Args;
use rayon::iter::{IntoParallelIterator, ParallelIterator};
use std::io::Cursor;
use std::path::{Path, PathBuf};

#[derive(Args)]
pub struct ScrambleArgs {
    /// The output directory that should contain the conversion result.
    /// If absent, output files will be in the same directory, but with the
    /// .plain.bdat/.scrambled.bdat extensions.
    #[arg(short, long)]
    out_dir: Option<String>,
    #[clap(flatten)]
    jobs: RayonPoolJobs,
}

pub fn scramble(input: InputData, args: ScrambleArgs) -> Result<()> {
    run(input, args, "scrambled.bdat", scramble_file)
}

pub fn unscramble(input: InputData, args: ScrambleArgs) -> Result<()> {
    run(input, args, "plain.bdat", unscramble_file)
}

fn run(
    input: InputData,
    args: ScrambleArgs,
    extension: &str,
    func: fn(PathBuf, PathBuf, &ProgressBarState) -> Result<()>,
) -> Result<()> {
    args.jobs.configure()?;

    let files = input
        .list_files("bdat", false)?
        .into_iter()
        .collect::<walkdir::Result<Vec<_>>>()?;

    let base_path = crate::util::get_common_denominator(&files);
    let out_dir = args.out_dir.map(PathBuf::from);

    let out_file_name = |file: &PathBuf| match out_dir.as_ref() {
        Some(out_dir) => {
            let relative_path = file
                .strip_prefix(&base_path)
                .unwrap()
                .parent()
                .unwrap_or_else(|| Path::new(""));

            let out_dir = out_dir.join(relative_path);
            std::fs::create_dir_all(&out_dir).context("Could not create output directory")?;

            Ok::<_, anyhow::Error>(out_dir.join(file.file_name().unwrap()))
        }
        None => Ok(file.with_extension(extension)),
    };

    let progress = ProgressBarState::new("Files", "Tables", files.len());
    progress.master_bar.inc(0);

    let res = files
        .into_par_iter()
        .panic_fuse()
        .map(|file| {
            let out = out_file_name(&file)?;
            func(file, out, &progress)?;
            progress.master_bar.inc(1);
            Ok(())
        })
        .find_any(|r: &anyhow::Result<()>| r.is_err());

    progress.master_bar.finish();

    if let Some(r) = res {
        r?;
    }

    Ok(())
}

fn unscramble_file(path_in: PathBuf, path_out: PathBuf, progress: &ProgressBarState) -> Result<()> {
    let mut bytes = std::fs::read(path_in)?;
    let BdatVersion::Legacy(version) = bdat::detect_bytes_version(&bytes)? else {
        return Err(Error::NotLegacy.into());
    };
    let cursor = Cursor::new(&bytes);
    let header = match version {
        LegacyVersion::Switch => FileHeader::read::<_, SwitchEndian>(cursor),
        LegacyVersion::X | LegacyVersion::Wii => FileHeader::read::<_, WiiEndian>(cursor),
    }?;

    let table_bar = progress.add_child(header.table_count);
    table_bar.inc(0);

    header.for_each_table_mut(&mut bytes, |table| {
        let header = match version {
            LegacyVersion::Switch => {
                TableHeader::read::<SwitchEndian>(Cursor::new(&table), version)
            }
            LegacyVersion::X | LegacyVersion::Wii => {
                TableHeader::read::<WiiEndian>(Cursor::new(&table), version)
            }
        }?;
        if let ScrambleType::None = header.scramble_type {
            progress.println(format!(
                "Note: skipping table {} (not scrambled)",
                header.read_name(table)?
            ))?;
            return Ok(());
        }
        header.unscramble_data(table);
        table_bar.inc(1);
        Ok::<_, anyhow::Error>(())
    })?;

    table_bar.finish();
    progress.remove_child(&table_bar);

    std::fs::write(path_out, bytes)?;
    Ok(())
}

fn scramble_file(path_in: PathBuf, path_out: PathBuf, progress: &ProgressBarState) -> Result<()> {
    let file_name = path_in.file_name().unwrap().to_string_lossy();
    let mut bytes = std::fs::read(&path_in)?;
    let BdatVersion::Legacy(version) = bdat::detect_bytes_version(&bytes)? else {
        return Err(Error::NotLegacy.into());
    };
    let cursor = Cursor::new(&bytes);
    let wii_endian = match version {
        LegacyVersion::Wii | LegacyVersion::X => true,
        LegacyVersion::Switch => false,
    };
    let header = match wii_endian {
        true => FileHeader::read::<_, WiiEndian>(cursor),
        false => FileHeader::read::<_, SwitchEndian>(cursor),
    }?;

    let table_bar = progress.add_child(header.table_count);
    table_bar.inc(0);

    let mut table_idx = 0;

    header.for_each_table_mut(&mut bytes, |table| {
        let header = match wii_endian {
            true => TableHeader::read::<WiiEndian>(Cursor::new(&table), version),
            false => TableHeader::read::<SwitchEndian>(Cursor::new(&table), version),
        }?;
        table_idx += 1;
        if let ScrambleType::Scrambled(_) = header.scramble_type {
            progress.println(format!(
                "Note: skipping table {} from {} (already scrambled)",
                table_idx, file_name
            ))?;
            return Ok(());
        }
        match wii_endian {
            true => header.scramble_data::<WiiEndian>(table),
            false => header.scramble_data::<SwitchEndian>(table),
        }
        table_bar.inc(1);
        Ok::<_, anyhow::Error>(())
    })?;

    table_bar.finish();
    progress.remove_child(&table_bar);

    std::fs::write(path_out, bytes)?;
    Ok(())
}
