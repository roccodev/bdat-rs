use std::{
    borrow::Cow,
    collections::HashMap,
    fs::{File, OpenOptions},
    io::{BufWriter, Read},
    path::{Path, PathBuf},
};

use bdat::{
    io::BdatVersion,
    types::{Label, RawTable, ValueType},
};
use serde::{Deserialize, Serialize};

/// Defines the structure of a BDAT file, so it can
/// be re-serialized properly.
#[derive(Serialize, Deserialize)]
pub struct FileSchema {
    pub file_name: String,
    pub version: BdatVersion,
    tables: Vec<String>,
    type_overrides: Option<HashMap<ColumnPath, ValueType>>,
}

#[derive(Serialize, Deserialize, PartialEq, Eq, Hash, Clone)]
#[serde(into = "String")]
struct ColumnPath {
    table: Option<Label>,
    column: Label,
}

pub trait AsFileName {
    fn as_file_name(&self) -> Cow<str>;
}

impl FileSchema {
    pub fn new(file_name: String, version: BdatVersion, calc_types: bool) -> Self {
        Self {
            file_name,
            version,
            tables: Vec::new(),
            type_overrides: calc_types.then(|| HashMap::default()),
        }
    }

    pub fn read(reader: impl Read) -> anyhow::Result<Self> {
        Ok(serde_json::from_reader(reader)?)
    }

    /// Registers a table in the file schema
    pub fn feed_table(&mut self, table: &RawTable) {
        self.tables
            .extend(table.name.clone().map(|l| l.as_file_name().to_string()));
        if let Some(type_map) = &mut self.type_overrides {
            for col in &table.columns {
                type_map.insert(
                    ColumnPath {
                        table: table.name.clone(),
                        column: col.label.clone(),
                    },
                    col.ty,
                );
            }
        }
    }

    /// Attempts to find all deserialized table files, from the paths defined by the
    /// file schema.
    pub fn find_table_files(&self, base_dir: &Path, extension: &str) -> Vec<PathBuf> {
        let mut files = Vec::with_capacity(self.tables.len());

        for label in self
            .tables
            .iter()
            .chain(std::iter::once(&self.file_name.clone()))
        {
            let path = base_dir.join(format!("{}.{extension}", label));
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

    /// Returns the stored type for a column. If the schema doesn't have an entry for it,
    /// the column's original type (as defined by the table data) is returned instead.
    /// If no column type was stored in table data, [`None`] is returned.
    pub fn get_column_type(
        &self,
        table_name: Option<Label>,
        col_name: Label,
        default: Option<ValueType>,
    ) -> Option<ValueType> {
        self.type_overrides
            .as_ref()
            .and_then(|m| {
                m.get(&ColumnPath {
                    table: table_name,
                    column: col_name,
                })
            })
            .copied()
            .or(default)
    }

    /// Returns the number of tables defined in this file.
    pub fn table_count(&self) -> usize {
        self.tables.len()
    }

    /// Writes the file schema to a file.
    pub fn write(&self, base_dir: impl AsRef<Path>) -> anyhow::Result<()> {
        let file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(
                base_dir
                    .as_ref()
                    .join(format!("{}.bschema", self.file_name)),
            )?;
        let writer = BufWriter::new(file);
        serde_json::to_writer(writer, self).unwrap();
        Ok(())
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

impl From<ColumnPath> for String {
    fn from(path: ColumnPath) -> Self {
        format!(
            "{}/{:+}",
            path.table.map(|l| format!("{:+}", l)).unwrap_or_default(),
            path.column
        )
    }
}

impl From<String> for ColumnPath {
    fn from(s: String) -> Self {
        todo!()
    }
}
