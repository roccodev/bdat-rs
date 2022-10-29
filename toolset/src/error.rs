use std::fmt::Display;

use bdat::Label;

#[derive(Debug)]
pub struct OptLabel(Option<Label>);

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Missing required argument: {0}")]
    MissingRequiredArgument(&'static str),
    #[error("Unsupported file type '{0}'")]
    UnknownFileType(String),
    #[error("No schema files found, please run 'extract' without '--no-schema'")]
    DeserMissingSchema,
    #[error("Table {0} is missing type information, please run 'extract' without '-u', or add types manually")]
    DeserMissingTypeInfo(OptLabel),
    #[error("Row {0} does not have entries for all columns")]
    DeserIncompleteRow(usize),
}

impl Display for OptLabel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.0 {
            Some(l) => l.fmt(f),
            None => write!(f, "<Unnamed>"),
        }
    }
}

impl<L> From<L> for OptLabel
where
    L: Into<Option<Label>>,
{
    fn from(label: L) -> Self {
        Self(label.into())
    }
}
