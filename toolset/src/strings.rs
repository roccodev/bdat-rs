use crate::{
    filter::{BdatFileFilter, Filter, FilterArg},
    util::op_result::OpResult,
    InputData,
};
use anyhow::{Context, Result};
use bdat::{
    compat::{CompatColumnRef, CompatTable},
    Cell, Label, ValueType,
};
use clap::Args;
use itertools::Itertools;
use std::borrow::Borrow;

#[derive(Args)]
pub struct StringsArgs {
    /// Don't print table names
    #[arg(long)]
    no_table_names: bool,
    /// Don't print column names
    #[arg(long)]
    no_column_names: bool,
    /// Don't print strings from row values
    #[arg(long)]
    no_rows: bool,
    /// Don't print debug strings (XC3+)
    #[arg(long)]
    no_debug: bool,

    /// Only print strings longer than this value
    #[arg(short = 'n', long, default_value = "1")]
    min_length: usize,
    /// Only print strings shorter than this value
    #[arg(short = 'N', long)]
    max_length: Option<usize>,

    /// Convert list/flags column names to the XCXDE format
    #[arg(long)]
    convert: bool,

    /// Only check these tables. If absent, prints strings from all tables.
    #[arg(short, long)]
    tables: Vec<String>,
    /// Only print row values from these columns. If absent, prints row values from all columns.
    #[arg(short, long)]
    columns: Vec<String>,

    #[clap(flatten)]
    input: InputData,
}

pub fn run(args: StringsArgs) -> Result<()> {
    let mut hash_table = args.input.load_hashes()?;
    let table_filter: Filter = args.tables.iter().cloned().map(FilterArg).collect();
    let column_filter: Filter = args.columns.iter().cloned().map(FilterArg).collect();

    let mut op_result = OpResult::default();
    for file in args.input.list_files(BdatFileFilter, false)? {
        let path = file?;
        op_result.start_file(path.to_string_lossy().to_string());
        let mut file = std::fs::read(&path)?;
        let tables = args
            .input
            .game_from_bytes(&file)?
            .from_bytes(&mut file, &mut hash_table, &mut op_result)
            .with_context(|| format!("Could not parse BDAT tables ({})", path.to_string_lossy()))?;
        for table in tables {
            if !table_filter.contains(&table.name()) {
                continue;
            }
            print_table(&table, &args, &column_filter);
        }
        op_result.end_file();
    }

    op_result.print();

    Ok(())
}

fn print_table(table: &CompatTable, args: &StringsArgs, col_filter: &Filter) {
    if !args.no_table_names {
        print_label(table.name(), args);
    }
    if !args.no_column_names {
        for col in table.columns() {
            print_column_name(col, args);
        }
    }
    if !args.no_rows {
        let string_fields = table
            .columns()
            .filter(|c| {
                col_filter.contains(&c.label())
                    && (c.value_type() == ValueType::String
                        || c.value_type() == ValueType::DebugString && !args.no_debug)
            })
            .collect_vec();
        if !string_fields.is_empty() {
            for row in table.rows() {
                for field in &string_fields {
                    match row.get(field.label()) {
                        Cell::Single(s) => print(s.as_str(), args),
                        Cell::List(l) => {
                            for v in l {
                                print(v.as_str(), args)
                            }
                        }
                        _ => continue,
                    }
                }
            }
        }
    }
}

#[inline]
fn print_column_name(col: CompatColumnRef, args: &StringsArgs) {
    let name = col.label();
    if args.convert {
        if col.count() > 1 {
            // List cells
            for i in 0..col.count() {
                print(format!("{}[{i}]", name), args);
            }
            return;
        }
        if !col.flags().is_empty() {
            // Flag cells
            for flag in col.flags() {
                print(format!("{}({})", name, flag.label()), args);
            }
            return;
        }
    }
    print_label(name, args);
}

#[inline]
fn print_label<'a>(label: impl Borrow<Label<'a>>, args: &StringsArgs) {
    if let Label::String(s) = label.borrow() {
        print(s.as_ref(), args)
    }
}

#[inline]
fn print(s: impl Borrow<str>, args: &StringsArgs) {
    let s = s.borrow();
    if args.max_length.is_none_or(|max| s.len() <= max) && s.len() >= args.min_length {
        println!("{}", s);
    }
}
