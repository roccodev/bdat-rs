//! Crate-private traits that help reduce boilerplate or generalize implementations, but aren't
//! exposed in the public API.

use crate::{LegacyFlag, ValueType};

pub trait Table<'buf> {
    type Id: From<u8>;
    type Name;
    type Row;
    type BuilderRow;
    type Column: Column;
    type BuilderColumn: Column;
}

pub trait Column {
    type Name: Clone + Ord + PartialEq;

    /// Returns this column's name.
    fn clone_label(&self) -> Self::Name;

    /// Returns this column's type.
    fn value_type(&self) -> ValueType;
}

pub trait ColumnSerialize {
    fn ser_value_type(&self) -> ValueType;
    fn ser_flags(&self) -> &[LegacyFlag];
}
