use clap::{Parser, Subcommand};

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
    Convert {
        #[arg(short, long)]
        out_file: Option<String>,
    },
    /// Print info about the structure of the BDAT file and the tables contained within
    Info {
        /// Only check these tables. If absent, returns data from all tables.
        #[arg(short, long)]
        tables: Vec<String>,
    },
}

pub struct InputData {
    in_file: String,
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match &cli.command {
        Some(Commands::Info { .. }) => info::get_info(InputData { in_file: cli.file }),
        _ => Ok(()),
    }
}
