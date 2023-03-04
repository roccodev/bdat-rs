use std::hash::{Hash, Hasher};
use std::{
    borrow::Cow,
    cmp::Ordering,
    collections::BTreeMap,
    fs::File,
    io::BufReader,
    path::{Path, PathBuf},
};

use anyhow::Result;
use clap::Args;
use indicatif::ProgressBar;
use itertools::Itertools;
use rayon::{iter::Either, prelude::*};

use bdat::{Cell, Label, RawTable, RowRef, SwitchEndian};

use crate::{hash::MurmurHashSet, InputData};

#[derive(Args)]
pub struct DiffArgs {
    /// Paths to the "old" BDAT files. Syntax: "--old <path1> --old <path2> ...".
    /// For the "new" BDAT files, use the global FILES argument.
    #[arg(long = "old", action = clap::ArgAction::Append, required = true)]
    old_files: Vec<String>,
    /// Expand table contents for added and removed tables. Table contents are
    /// always expanded for tables that were only changed.
    #[arg(long = "expand", default_value_t = true)]
    expand_tables: bool,
    /// Don't print file names.
    #[arg(long)]
    no_file_names: bool,
}

struct TableWithSource<'f, 't> {
    table: RawTable<'t>,
    source_file: &'f Path,
}

#[derive(Clone)]
struct PathDiff<'p> {
    old: &'p Path,
    new: &'p Path,
}

struct RowChanges<'t, 'tb> {
    row_id: usize,
    old_hash: Option<Label>,
    new_hash: Option<Label>,
    changes: Vec<ColumnChange<'t, 'tb>>,
}

struct ColumnChange<'t, 'tb> {
    label: &'t Label,
    added: bool,
    value: &'t Cell<'tb>,
}

struct ValueOrderedLabel(Label);

pub fn run_diff(input: InputData, args: DiffArgs) -> Result<()> {
    let progress = ProgressBar::new(3)
        .with_style(crate::convert::build_progress_style("Diff", true))
        .with_message(" (Reading files)");
    let new_files = input.list_files("bdat", !args.no_file_names)?.into_iter();
    let old_files = InputData {
        files: args.old_files,
        hashes: None,
    };
    let old_files = old_files
        .list_files("bdat", !args.no_file_names)?
        .into_iter();
    let hash_table = input.load_hashes()?;

    let files_to_read = new_files
        .map(|f| f.map(|f| (f, true)))
        .chain(old_files.map(|f| f.map(|f| (f, false))))
        .collect::<walkdir::Result<Vec<_>>>()?;
    progress.inc(1);
    progress.set_message(" (Parsing tables)");

    let working_directory = std::env::current_dir()?;

    // Read old & new files concurrently
    let (old_tables, new_tables) = files_to_read
        .par_iter()
        .flat_map(|(file, new)| {
            let reader = BufReader::new(File::open(file)?);
            let mut tables = bdat::from_reader::<_, SwitchEndian>(reader).and_then(|mut f| {
                Ok(f.get_tables()?
                    .into_iter()
                    .map(|table| TableWithSource {
                        table,
                        source_file: file,
                    })
                    .collect_vec())
            })?;
            for table in &mut tables {
                hash_table.convert_all(&mut table.table);
            }
            Ok::<(Vec<TableWithSource>, bool), anyhow::Error>((tables, *new))
        })
        .partition_map::<Vec<Result<_>>, Vec<Result<_>>, _, Result<_>, Result<_>>(
            |(tables, new)| match new {
                true => Either::Right(Ok(tables)),
                false => Either::Left(Ok(tables)),
            },
        );
    let (old_tables, new_tables): (
        BTreeMap<ValueOrderedLabel, TableWithSource>,
        BTreeMap<ValueOrderedLabel, TableWithSource>,
    ) = (
        old_tables
            .into_iter()
            .flatten_ok()
            .map_ok(|t| (ValueOrderedLabel(t.table.name().unwrap().clone()), t))
            .try_collect()?,
        new_tables
            .into_iter()
            .flatten_ok()
            .map_ok(|t| (ValueOrderedLabel(t.table.name().unwrap().clone()), t))
            .try_collect()?,
    );
    progress.inc(1);

    let added = new_tables
        .iter()
        .filter_map(|(name, table)| (!old_tables.contains_key(name)).then_some(table));
    let removed = old_tables
        .iter()
        .filter_map(|(name, table)| (!new_tables.contains_key(name)).then_some(table));

    progress.inc(1);
    progress.set_message(" (Processing result)");

    println!("------------\nAdded Tables\n------------");
    added.for_each(|table| {
        if args.no_file_names {
            println!("+ Table \"{}\"", table.table.name().unwrap());
        } else {
            println!(
                "+ Table \"{}\" (new: {})",
                table.table.name().unwrap(),
                table
                    .source_file
                    .strip_prefix(&working_directory)
                    .unwrap_or(table.source_file)
                    .display()
            )
        }
    });

    println!("\n--------------\nRemoved Tables\n--------------");
    removed.for_each(|table| {
        if args.no_file_names {
            println!("- Table \"{}\"", table.table.name().unwrap());
        } else {
            println!(
                "- Table \"{}\" (old: {})",
                table.table.name().unwrap(),
                table
                    .source_file
                    .strip_prefix(&working_directory)
                    .unwrap_or(table.source_file)
                    .display()
            )
        }
    });

    println!("\n--------------\nChanged Tables\n--------------");
    for (ref l @ ValueOrderedLabel(ref name), table) in old_tables.into_iter() {
        let new_table = match new_tables.get(l) {
            Some(table) => table,
            None => continue,
        };

        let row_changes = new_table
            .table
            .rows()
            .flat_map(|new_row| {
                let id = new_row.id();
                let old_row = table.table.get_row(id);
                RowChanges::diff(
                    id,
                    old_row
                        .as_ref()
                        .and_then(|t| t.as_ref().id_hash())
                        .map(Label::Hash),
                    new_row.id_hash().map(Label::Hash),
                    old_row.map(|row| row),
                    Some(new_table.table.get_row(id).unwrap()),
                )
            })
            .collect_vec();
        if !row_changes.is_empty() {
            let path_diff = table.get_path_diff(new_table);
            let path_diff = path_diff.to_distinguishable();
            if args.no_file_names {
                println!("\nTable \"{name}\"");
            } else {
                println!(
                    "\nTable \"{name}\" (old: {}, new: {}):",
                    path_diff.old.display(),
                    path_diff.new.display()
                );
            }
            for row_changed in row_changes {
                row_changed.print();
            }
        }
    }

    Ok(())
}

impl<'t, 'tb> RowChanges<'t, 'tb> {
    fn diff(
        row_id: usize,
        old_hash: Option<Label>,
        new_hash: Option<Label>,
        old: Option<RowRef<'t, 'tb>>,
        new: Option<RowRef<'t, 'tb>>,
    ) -> Option<Self> {
        let changed_cols: Vec<ColumnChange> = match (old, new) {
            (None, Some(new_row)) => new_row
                .table()
                .columns()
                .map(|col| (&col.label, true, new_row.get(&col.label).unwrap()).into())
                .collect(),
            (Some(old_row), None) => old_row
                .table()
                .columns()
                .map(|col| (&col.label, false, old_row.get(&col.label).unwrap()).into())
                .collect(),
            (Some(old_row), Some(new_row)) => {
                let (old_table, new_table) = (old_row.table(), new_row.table());
                let old_cols: MurmurHashSet<_> =
                    old_table.columns().map(|col| &col.label).collect();
                let new_cols: MurmurHashSet<_> =
                    new_table.columns().map(|col| &col.label).collect();

                let changed_cols = old_cols.intersection(&new_cols).filter_map(|col| {
                    let old_value = old_row.get(*col)?;
                    let new_value = new_row.get(*col)?;
                    (old_value != new_value).then_some((col, old_value, new_value))
                });

                new_cols
                    .difference(&old_cols)
                    .map(|&label| (label, true, old_row.get(label).unwrap()).into())
                    .chain(
                        old_cols
                            .difference(&new_cols)
                            .map(|&label| (label, false, new_row.get(label).unwrap()).into()),
                    )
                    .chain(changed_cols.flat_map(|(&label, old_val, new_val)| {
                        [
                            (label, false, old_val).into(),
                            (label, true, new_val).into(),
                        ]
                        .into_iter()
                    }))
                    .collect()
            }
            _ => unreachable!(),
        };

        (!changed_cols.is_empty()).then_some(Self {
            row_id,
            old_hash,
            new_hash,
            changes: changed_cols,
        })
    }

    fn print(self) {
        let removed = self
            .changes
            .iter()
            .filter_map(
                |ColumnChange {
                     label,
                     added,
                     value,
                 }| {
                    (!added).then(|| format!("{label}: {}", serde_json::to_string(value).unwrap()))
                },
            )
            .join(" / ");
        let added = self
            .changes
            .iter()
            .filter_map(
                |ColumnChange {
                     label,
                     added,
                     value,
                 }| {
                    added.then(|| format!("{label}: {}", serde_json::to_string(value).unwrap()))
                },
            )
            .join(" / ");

        if !removed.is_empty() {
            println!(
                "- Row {} ({}): {removed}",
                self.row_id,
                self.old_hash
                    .as_ref()
                    .map(|l| l.to_string())
                    .map(Cow::from)
                    .unwrap_or(Cow::Borrowed("N/A"))
            );
        }
        if !added.is_empty() {
            println!(
                "+ Row {} ({}): {added}",
                self.row_id,
                self.new_hash
                    .map(|l| l.to_string())
                    .map(Cow::from)
                    .unwrap_or(Cow::Borrowed("N/A"))
            );
        }
    }
}

impl<'f, 't> TableWithSource<'f, 't> {
    fn get_path_diff(&self, new: &TableWithSource<'f, 't>) -> PathDiff<'f> {
        PathDiff {
            old: self.source_file,
            new: new.source_file,
        }
    }
}

impl<'p> PathDiff<'p> {
    /// Calculates the rightmost common portion between the two paths, stopping at (and including) the first
    /// component that doesn't match (from the left).
    /// For example:  
    /// * `a/b/c.txt` and `d/b/c.txt` are already distinguishable.
    /// * `/usr/share/docs/lib.txt` and `/etc/lib.txt` are already distinguishable, as the first component
    ///   is already different.
    /// * `/mnt/ver1/test.bdat` and `/mnt/ver2/test.bdat` can be distinguished as `ver1/test.bdat` and `ver2/test.bdat`.
    ///
    /// **Important:** the paths must be canonical. (i.e. they cannot contain identifiers like '.' and '..')
    fn to_distinguishable(&'p self) -> Self {
        let Self { old, new } = self;
        if old == new {
            return self.clone();
        }
        let common = old
            .components()
            .zip(new.components())
            .take_while(|(old, new)| old == new)
            .map(|(_, new)| new)
            .collect::<PathBuf>();

        Self {
            old: old.strip_prefix(&common).unwrap(),
            new: new.strip_prefix(common).unwrap(),
        }
    }
}

impl<'t, 'tb> From<(&'t Label, bool, &'t Cell<'tb>)> for ColumnChange<'t, 'tb> {
    fn from(value: (&'t Label, bool, &'t Cell<'tb>)) -> Self {
        Self {
            label: value.0,
            added: value.1,
            value: value.2,
        }
    }
}

impl PartialOrd for ValueOrderedLabel {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.0.cmp_value(&other.0))
    }
}

impl Ord for ValueOrderedLabel {
    fn cmp(&self, other: &Self) -> Ordering {
        self.partial_cmp(other).unwrap()
    }
}

impl PartialEq for ValueOrderedLabel {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}

impl Eq for ValueOrderedLabel {}

impl Hash for ValueOrderedLabel {
    fn hash<H: Hasher>(&self, state: &mut H) {
        match &self.0 {
            Label::Hash(h) => state.write_u32(*h),
            Label::String(s) | Label::Unhashed(s) => state.write(s.as_bytes()),
        }
    }
}
