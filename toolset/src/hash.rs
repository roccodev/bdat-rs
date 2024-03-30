use std::{
    fs::File,
    io::{self, BufRead, BufReader},
};

use anyhow::Result;
use bdat::hash::murmur3_str;
use clap::{Args, ValueEnum};

#[derive(Args)]
pub struct HashArgs {
    #[arg(short, long, value_enum, default_value_t = Algorithm::Murmur32)]
    algorithm: Algorithm,
    #[clap(flatten)]
    output_settings: OutputSettings,
    #[clap(flatten)]
    input: Input,
}

#[derive(Args)]
#[group(required = true, multiple = false)]
struct Input {
    /// Read input from stdin (one string per line). Terminate your input with EOF or an empty
    /// line (double return)
    #[arg(short, long)]
    stdin: bool,
    /// Read input from a file (one string per line)
    #[arg(short, long)]
    file: Option<String>,
    /// Strings to hash (only allowed if `--stdin` and `--file` are both absent)
    strings: Vec<String>,
}

#[derive(Clone, Copy, ValueEnum)]
enum Algorithm {
    /// Murmur3 32-bit version, used by XC3
    Murmur32,
}

#[derive(Args)]
struct OutputSettings {
    /// If enabled, includes the input strings in the output. For example, an input of
    /// `test` will print `test = <BA6BD213>`
    #[arg(short, long)]
    keys: bool,
    /// The format to use for each hash value
    #[arg(short, long, value_enum, default_value_t = FormatMethod::HexBrackets)]
    method: FormatMethod,
}

#[derive(Clone, Copy, ValueEnum)]
enum FormatMethod {
    /// For 32-bit hashes, 8-digit hexadecimal output surrounded by angle brackets, e.g.
    /// <DEADBEEF>
    HexBrackets,
    /// For 32-bit hashes, 8-digit hexadecimal output prefixed by '0x', e.g. 0xDEADBEEF
    HexHex,
    /// For 32-bit hashes, 8-digit hexadecimal output, e.g. DEADBEEF
    Hex,
    /// Decimal output, e.g. 3735928559
    Decimal,
}

pub fn run(args: HashArgs) -> Result<()> {
    let input = if !args.input.strings.is_empty() {
        args.input.strings
    } else if let Some(file) = args.input.file {
        let file = File::open(file)?;
        let res: Result<Vec<_>> = BufReader::new(file)
            .lines()
            .map(|r| r.map_err(Into::into))
            .collect();
        res?
    } else {
        let stdin = io::stdin();
        let res: Result<Vec<_>> = stdin
            .lock()
            .lines()
            .take_while(|s| s.is_err() || s.as_ref().is_ok_and(|s| !s.is_empty()))
            .map(|r| r.map_err(Into::into))
            .collect();
        res?
    };

    for string in input {
        let hash = hash(args.algorithm, &string);
        print_result(&args.output_settings, &string, hash);
    }

    Ok(())
}

fn hash(algorithm: Algorithm, key: &str) -> u32 {
    match algorithm {
        Algorithm::Murmur32 => murmur3_str(key),
    }
}

fn print_result(settings: &OutputSettings, key: &str, hash: u32) {
    if settings.keys {
        print!("{key} = ");
    }
    match settings.method {
        FormatMethod::HexBrackets => println!("<{hash:08X}>"),
        FormatMethod::HexHex => println!("0x{hash:08X}"),
        FormatMethod::Hex => println!("{hash:08X}"),
        FormatMethod::Decimal => println!("<{hash}>"),
    }
}
