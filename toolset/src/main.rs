use std::borrow::Cow;
use std::path::Path;
use std::{fs::File, path::PathBuf};

use anyhow::{Context, Result};
use clap::{Args, Parser, Subcommand};
use convert::ConvertArgs;
use diff::DiffArgs;
use hash::HashNameTable;
use info::InfoArgs;
use itertools::Itertools;
use walkdir::WalkDir;

mod convert;
mod diff;
pub mod error;
pub mod filter;
pub mod hash;
mod info;
pub mod util;

#[derive(Parser)]
#[command(
    author,
    version,
    about,
    arg_required_else_help = true,
    subcommand_required = true
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    #[clap(flatten)]
    input: InputData,
}

#[derive(Subcommand)]
enum Commands {
    /// Extract tables from BDAT files
    Extract(ConvertArgs),
    /// Convert from extracted data to BDAT files
    Pack(ConvertArgs),
    /// Print info about the structure of the BDAT file and the tables contained within
    Info(InfoArgs),
    /// Print the differences between two BDAT dumps
    Diff(DiffArgs),
}

#[derive(Args)]
pub struct InputData {
    /// A file containing unhashed names, one in each line. If provided, all matched hashes will
    /// be replaced with the unhashed names.
    #[arg(long, global = true)]
    hashes: Option<String>,

    /// The input files. For "bdat-toolset diff", these are the "new" BDAT files.
    #[arg(global = true)]
    files: Vec<String>,
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Some(Commands::Info(args)) => info::get_info(cli.input, args),
        Some(Commands::Extract(args)) => convert::run_conversions(cli.input, args, true),
        Some(Commands::Pack(args)) => convert::run_conversions(cli.input, args, false),
        Some(Commands::Diff(args)) => diff::run_diff(cli.input, args),
        _ => Ok(()),
    }
}

impl InputData {
    pub fn list_files<'a, 'b: 'a, E: Into<Option<&'b str>>>(
        &'a self,
        extension: E,
        canonical_paths: bool,
    ) -> Result<impl IntoIterator<Item = walkdir::Result<PathBuf>> + 'a> {
        let extension = extension.into();
        let paths: Vec<_> = self
            .files
            .iter()
            .map(|name| {
                let mut root = Cow::Borrowed(Path::new(name));
                if canonical_paths {
                    root = Cow::Owned(root.canonicalize()?);
                }
                Ok::<_, anyhow::Error>(root)
            })
            .try_collect()?;

        Ok(paths.into_iter().flat_map(move |name| {
            WalkDir::new(name)
                .into_iter()
                .filter_map(move |p| match (p, extension) {
                    (Err(e), _) => Some(Err(e)),
                    (Ok(e), None) => Some(Ok(e.path().to_owned())),
                    (Ok(e), Some(ext)) => {
                        let path = e.path();
                        if let Some(path_ext) = path.extension() {
                            if matches!(path_ext.to_str(), Some(p) if p == ext) {
                                return Some(Ok(path.to_owned()));
                            }
                        }
                        None
                    }
                })
        }))
    }

    pub fn load_hashes(&self) -> Result<HashNameTable> {
        match &self.hashes {
            Some(path) => {
                let file = File::open(path).context("Could not open hashes file")?;
                Ok(HashNameTable::load_from_names(file)?)
            }
            None => Ok(HashNameTable::empty()),
        }
    }
}
