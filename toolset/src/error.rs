use std::fmt::Display;

use bdat::{Label, RowId, ValueType};

pub const MAX_DUPLICATE_COLUMNS: usize = 4;

#[derive(Debug)]
pub struct OptLabel(Option<Label<'static>>);

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Missing required argument: {0}")]
    MissingRequiredArgument(&'static str),
    #[error("Unsupported file type '{0}'")]
    UnknownFileType(String),
    #[error("Not a legacy BDAT file")]
    NotLegacy,
    #[error("Not a modern BDAT file")]
    NotModern,
    #[error("Schema error: {0}")]
    Schema(#[from] SchemaError),
    #[error("Table format error ({table}): {error}")]
    Format { table: OptLabel, error: FormatError },
}

#[derive(Debug, thiserror::Error)]
pub enum SchemaError {
    #[error("No schema files found, please run 'extract' without '--no-schema'")]
    MissingSchema,
    #[error(
        "Unsupported schema for file '{}', found version {}, expected one of {:?}. \
        Please update or run 'extract' again without '--no-schema'", _0.0, _0.1, _0.2
    )]
    UnsupportedSchema(Box<(String, usize, &'static [usize])>),
}

#[derive(Debug, thiserror::Error)]
pub enum FormatError {
    #[error("Missing type information, please run 'extract' without '-u', or add types manually")]
    MissingTypeInfo,
    #[error("Row {0} does not have entries for all columns")]
    IncompleteRow(RowId),
    #[error("Column name {0} is already present, and duplicates are not allowed.")]
    DuplicateColumn(OptLabel),
    #[error(
        "Column name {0} exceeds the maximum duplicate count of \
    {MAX_DUPLICATE_COLUMNS}. Please avoid using multiple columns with the same name."
    )]
    MaxDuplicateColumns(OptLabel),
    #[error("Columns with name {} differ in type ({:?} / {:?}). \
    Please avoid using multiple columns with the same name.", 
    _0.0, _0.1, _0.2)]
    DuplicateMismatch(Box<(OptLabel, ValueType, ValueType)>),
    #[error("Entry for row {0} is missing, was a row deleted without updating the IDs?")]
    MissingRow(usize),
}

impl FormatError {
    pub fn with_context(self, table_name: impl Into<OptLabel>) -> Error {
        Error::Format {
            table: table_name.into(),
            error: self,
        }
    }
}

impl Display for OptLabel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.0 {
            Some(l) => l.fmt(f),
            None => write!(f, "<Unnamed>"),
        }
    }
}

impl<'b, L> From<L> for OptLabel
where
    L: Into<Option<Label<'b>>>,
{
    fn from(label: L) -> Self {
        Self(label.into().map(Label::into_owned))
    }
}
