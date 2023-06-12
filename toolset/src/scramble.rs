use crate::util::{ProgressBarState, RayonPoolJobs};
use crate::InputData;
use anyhow::{Context, Result};
use bdat::legacy::{FileHeader, TableHeader};
use bdat::SwitchEndian;
use clap::Args;
use rayon::iter::{IntoParallelIterator, ParallelIterator};
use std::io::Cursor;
use std::path::{Path, PathBuf};

#[derive(Args)]
pub struct ScrambleArgs {
    /// The output directory that should contain the conversion result.
    /// If absent, output files will be in the same directory, but with the
    /// .plain.bdat extension.
    #[arg(short, long)]
    out_dir: Option<String>,
    #[clap(flatten)]
    jobs: RayonPoolJobs,
}

pub fn scramble(input: InputData, args: ScrambleArgs) -> Result<()> {
    Ok(())
}

pub fn unscramble(input: InputData, args: ScrambleArgs) -> Result<()> {
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
        None => Ok(file.with_extension("plain.bdat")),
    };

    let progress = ProgressBarState::new("Files", "Tables", files.len());
    progress.master_bar.inc(0);

    let res = files
        .into_par_iter()
        .panic_fuse()
        .map(|file| {
            let out = out_file_name(&file)?;
            unscramble_file(file, out, &progress)?;
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
    let cursor = Cursor::new(&bytes);
    // TODO: endianness
    let header = FileHeader::read::<_, SwitchEndian>(cursor)?;

    let table_bar = progress.add_child(header.table_count);
    table_bar.inc(0);

    header.for_each_table_mut(&mut bytes, |table| {
        let header = TableHeader::read::<SwitchEndian>(Cursor::new(&table))?;
        header.unscramble_data(table);
        table[4] = 0; // Change scramble type to none
        table_bar.inc(1);
        Ok::<_, anyhow::Error>(())
    })?;

    table_bar.finish();
    progress.remove_child(&table_bar);

    std::fs::write(path_out, bytes)?;
    Ok(())
}
