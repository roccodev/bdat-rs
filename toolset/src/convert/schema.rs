use std::{
    borrow::Cow,
    fs::OpenOptions,
    io::{BufWriter, Read},
    path::{Path, PathBuf},
};

use bdat::{
    io::BdatVersion,
    types::{Label, Table},
};
use serde::{Deserialize, Serialize};

/// Defines the structure of a BDAT file, so it can
/// be re-serialized properly.
#[derive(Serialize, Deserialize)]
pub struct FileSchema {
    pub file_name: String,
    pub version: BdatVersion,
    tables: Vec<String>,
}

pub trait AsFileName {
    fn as_file_name(&self) -> Cow<str>;
}

impl FileSchema {
    pub fn new(file_name: String, version: BdatVersion) -> Self {
        Self {
            file_name,
            version,
            tables: Vec::new(),
        }
    }

    pub fn read(reader: impl Read) -> anyhow::Result<Self> {
        Ok(serde_json::from_reader(reader)?)
    }

    /// Registers a table in the file schema
    pub fn feed_table(&mut self, table: &Table) {
        self.tables.extend(table.name().map(|l| l.to_string()));
    }

    /// Attempts to find all deserialized table files, from the paths defined by the
    /// file schema.
    pub fn find_table_files(&self, base_dir: &Path, extension: &str) -> Vec<(Label, PathBuf)> {
        let mut files = Vec::with_capacity(self.tables.len());

        for label in self
            .tables
            .iter()
            .chain(std::iter::once(&self.file_name.clone()))
        {
            let label = Label::parse(label.clone(), false);
            let path = base_dir.join(format!("{}.{extension}", label.as_file_name()));
            if path.is_file() {
                files.push((label, path));
            }
        }

        files
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
