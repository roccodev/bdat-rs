use std::{
    borrow::Cow,
    fs::OpenOptions,
    io::{BufWriter, Read},
    path::{Path, PathBuf},
};

use crate::error::{Error, SchemaError};
use bdat::{BdatVersion, Label, Table, Utf};
use serde::{Deserialize, Serialize};

/// Incremental format version, used to determine schema compatibility.
const FORMAT_VERSION: usize = 2;
/// Currently supported format versions (backwards compatibility)
const SUPPORTED_VERSIONS: &[usize] = &[FORMAT_VERSION, 1];

/// Defines the structure of a BDAT file, so it can
/// be re-serialized properly.
#[derive(Serialize, Deserialize)]
pub struct FileSchema {
    pub file_name: String,
    pub version: BdatVersion,
    #[serde(default)]
    pub format_version: usize,
    tables: Vec<String>,
}

pub trait AsFileName {
    fn as_file_name(&self) -> Utf;
}

impl FileSchema {
    pub fn new(file_name: String, version: BdatVersion) -> Self {
        Self {
            file_name,
            version,
            format_version: FORMAT_VERSION,
            tables: Vec::new(),
        }
    }

    pub fn read(reader: impl Read) -> anyhow::Result<Self> {
        let schema: FileSchema = serde_json::from_reader(reader)?;
        if !SUPPORTED_VERSIONS.contains(&schema.format_version) {
            return Err(Error::from(SchemaError::UnsupportedSchema(Box::new((
                schema.file_name,
                schema.format_version,
                SUPPORTED_VERSIONS,
            ))))
            .into());
        }
        Ok(schema)
    }

    /// Registers a table in the file schema
    pub fn feed_table(&mut self, table: &Table) {
        self.tables.push(table.name().to_string());
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

impl<'b> AsFileName for Label<'b> {
    fn as_file_name(&self) -> Utf {
        match self {
            // {:+} displays hashed names without brackets (<>)
            l @ Label::Hash(_) => Cow::Owned(format!("{:+}", l)),
            Label::String(s) => Cow::Borrowed(s),
        }
    }
}
