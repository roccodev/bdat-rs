use bdat::{
    Cell, ColumnBuilder, ColumnDef, FlagDef, Label, LegacyTable, LegacyRow, 
    LegacyTableBuilder, Value, ValueType,
};

pub fn duplicate_table_create() -> LegacyTable<'static> {
    let flag = FlagDef::new_bit("Flag1", 0);

    LegacyTableBuilder::with_name(Label::String("Test".to_string()))
        .add_column(
            ColumnBuilder::new(ValueType::SignedInt, "Label1".to_string().into())
                .set_flags(vec![flag.clone()])
                .build(),
        )
        .add_column(
            ColumnBuilder::new(ValueType::SignedInt, "Label1".to_string().into())
                .set_flags(vec![flag])
                .build(),
        )
        .add_column(ColumnDef::new(
            ValueType::SignedByte,
            "Label2".to_string().into(),
        ))
        .add_row(LegacyRow::new(
            vec![
                Cell::Flags(vec![1]),
                Cell::Flags(vec![1]),
                Cell::Single(Value::SignedByte(2)),
            ],
        ))
        .add_row(LegacyRow::new(
            vec![
                Cell::Flags(vec![0]),
                Cell::Flags(vec![0]),
                Cell::Single(Value::SignedByte(-4)),
            ],
        ))
        .build()
}
