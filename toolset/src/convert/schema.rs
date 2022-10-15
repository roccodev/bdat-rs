use std::{
    borrow::Cow,
    collections::HashMap,
    path::{Path, PathBuf},
};

use bdat::types::{Label, RawTable, ValueType};
use serde::{Deserialize, Serialize};

/// Defines the structure of a BDAT file, so it can
/// be re-serialized properly.
#[derive(Serialize, Deserialize)]
struct FileSchema {
    file_name: String,
    tables: Vec<Label>,
    type_overrides: HashMap<ColumnPath, ValueType>,
}

#[derive(Serialize, Deserialize, PartialEq, Eq, Hash)]
struct ColumnPath {
    table: Option<Label>,
    column: Label,
}

pub trait AsFileName {
    fn as_file_name(&self) -> Cow<str>;
}

impl FileSchema {
    pub fn new(
        file_name: String,
        tables: impl IntoIterator<Item = RawTable>,
        calc_types: bool,
    ) -> Self {
        let (tables, type_map) = tables
            .into_iter()
            .map(|t| {
                (
                    t.name,
                    t.columns
                        .iter()
                        .map(|c| (c.label.clone(), c.ty))
                        .collect::<Vec<_>>(),
                )
            })
            .fold(
                (vec![], HashMap::new()),
                |(mut labels, mut type_map), (t_lbl, t_types)| {
                    for (col, ty) in t_types {
                        type_map.insert(
                            ColumnPath {
                                table: t_lbl.clone(),
                                column: col.clone(),
                            },
                            ty,
                        );
                    }
                    labels.extend(t_lbl);
                    (labels, type_map)
                },
            );
        Self {
            file_name,
            tables,
            type_overrides: type_map,
        }
    }

    pub fn find_table_files(&self, base_dir: &Path, extension: &str) -> Vec<PathBuf> {
        let mut files = Vec::with_capacity(self.tables.len());

        for label in self
            .tables
            .iter()
            .chain(std::iter::once(&Label::String(self.file_name.clone())))
        {
            let path = base_dir.join(format!("{}.{extension}", label.as_file_name()));
            if path.is_file() {
                files.push(path);
            } else {
                eprintln!(
                    "[Warn] Table file {} (required for {}) not found.",
                    path.to_string_lossy(),
                    self.file_name
                );
            }
        }

        files
    }
}

impl AsFileName for Label {
    fn as_file_name(&self) -> Cow<str> {
        match self {
            // {:+} displays hashed names without brackets (<>)
            l @ Label::Hash(_) => Cow::Owned(format!("{:+}", l)),
            Label::String(s) | Label::Unhashed(s) => Cow::Borrowed(s),
        }
    }
}
