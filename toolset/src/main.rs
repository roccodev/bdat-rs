use std::{
    borrow::Cow,
    path::{Path, PathBuf},
};

use clap::{Parser, Subcommand};
use convert::ConvertArgs;
use info::InfoArgs;
use walkdir::WalkDir;

mod convert;
pub mod filter;
mod info;

#[derive(Parser)]
#[command(author, version, about)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    /// A file containing unhashed names, one in each line. If provided, all matched hashes will
    /// be replaced with the unhashed names.
    #[arg(long, global = true)]
    hashes: Option<String>,

    /// The input files
    #[arg(global = true)]
    files: Vec<String>,
}

#[derive(Subcommand)]
enum Commands {
    /// Convert to and from BDAT files
    Convert(ConvertArgs),
    /// Print info about the structure of the BDAT file and the tables contained within
    Info(InfoArgs),
}

pub struct InputData {
    in_file: Vec<String>,
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Some(Commands::Info(args)) => info::get_info(InputData { in_file: cli.files }, args),
        Some(Commands::Convert(args)) => {
            convert::run_conversions(InputData { in_file: cli.files }, args)
        }
        _ => Ok(()),
    }
}

impl InputData {
    pub fn list_files<'a, 'b: 'a, E: Into<Option<&'b str>>>(
        &'a self,
        extension: E,
    ) -> impl IntoIterator<Item = walkdir::Result<PathBuf>> + 'a {
        let extension = extension.into();
        self.in_file.iter().flat_map(move |name| {
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
        })
    }
}
