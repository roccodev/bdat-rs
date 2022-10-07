use clap::{Parser, Subcommand};
use convert::ConvertArgs;
use info::InfoArgs;

mod convert;
pub mod filter;
mod info;

#[derive(Parser)]
#[command(author, version, about)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    file: String,
}

#[derive(Subcommand)]
enum Commands {
    /// Convert to and from BDAT files
    Convert(ConvertArgs),
    /// Print info about the structure of the BDAT file and the tables contained within
    Info(InfoArgs),
}

pub struct InputData {
    in_file: String,
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Some(Commands::Info(args)) => info::get_info(InputData { in_file: cli.file }, args),
        Some(Commands::Convert(args)) => {
            convert::run_conversions(InputData { in_file: cli.file }, args)
        }
        _ => Ok(()),
    }
}
