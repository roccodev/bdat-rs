use crate::{LegacyFlag, ValueType};

pub trait ColumnSerialize {
    fn ser_value_type(&self) -> ValueType;
    fn ser_flags(&self) -> &[LegacyFlag];
}
