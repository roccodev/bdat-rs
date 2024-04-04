use bdat::legacy::{LegacyColumnBuilder, LegacyRow, LegacyTable, LegacyTableBuilder};
use bdat::{Cell, LegacyFlag, Value, ValueType};

pub fn duplicate_table_create() -> LegacyTable<'static> {
    let flag = LegacyFlag::new_bit("Flag1", 0);

    LegacyTableBuilder::with_name("Test")
        .add_column(
            LegacyColumnBuilder::new(ValueType::SignedInt, "Label1".to_string().into())
                .set_flags(vec![flag.clone()])
                .build(),
        )
        .add_column(
            LegacyColumnBuilder::new(ValueType::SignedInt, "Label1".to_string().into())
                .set_flags(vec![flag])
                .build(),
        )
        .add_column(LegacyColumnBuilder::new(
            ValueType::SignedByte,
            "Label2".to_string().into(),
        ))
        .add_row(LegacyRow::new(vec![
            Cell::Flags(vec![1]),
            Cell::Flags(vec![1]),
            Cell::Single(Value::SignedByte(2)),
        ]))
        .add_row(LegacyRow::new(vec![
            Cell::Flags(vec![0]),
            Cell::Flags(vec![0]),
            Cell::Single(Value::SignedByte(-4)),
        ]))
        .build()
}
