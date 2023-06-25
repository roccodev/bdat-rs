//! Tests the hash table found in legacy BDAT files.

use bdat::legacy::LegacyHashTable;
use bdat::{
    BdatVersion, ColumnBuilder, ColumnDef, FlagDef, SwitchEndian, Table, TableBuilder, ValueType,
    WiiEndian,
};
use byteorder::ByteOrder;
use std::collections::HashSet;
use std::ffi::CStr;

#[test]
fn hash_table_legacy() {
    test_table(&create_table(), BdatVersion::Legacy);
}

#[test]
fn hash_table_xcx() {
    test_table(&create_table(), BdatVersion::LegacyX);
}

fn create_table<'b>() -> Table<'b> {
    TableBuilder::new()
        .set_name(Some("Table1".to_string().into()))
        .add_column(ColumnDef::new(
            ValueType::SignedByte,
            "ColumnXX1".to_string().into(),
        ))
        .add_column(ColumnDef::new(
            ValueType::SignedByte,
            "ColumnXX2".to_string().into(),
        ))
        .add_column(ColumnDef::new(
            ValueType::String,
            "TotallyDifferentColumn".to_string().into(),
        ))
        .add_column(
            ColumnBuilder::new(ValueType::SignedByte, "ColumnXXFlags".to_string().into())
                .set_flags(vec![
                    FlagDef::new_bit("Bit1", 0),
                    FlagDef::new_bit("Bit2", 1),
                    FlagDef::new_bit("ColumnXX4", 2),
                ])
                .build(),
        )
        .build()
}

fn test_table(table: &Table, version: BdatVersion) {
    let written = match version {
        BdatVersion::Legacy => bdat::legacy::to_vec::<SwitchEndian>([table], version).unwrap(),
        BdatVersion::LegacyX => bdat::legacy::to_vec::<WiiEndian>([table], version).unwrap(),
        _ => unreachable!(),
    };

    let table_bytes = &written[12..];

    for col in table
        .columns()
        .map(|c| c.label().to_string_convert().to_string())
        .chain(
            table
                .columns()
                .flat_map(|c| c.flags())
                .map(|f| f.label().to_string()),
        )
    {
        assert!(
            match version {
                BdatVersion::Legacy => find_col_def::<SwitchEndian>(table_bytes, &col),
                BdatVersion::LegacyX => find_col_def::<WiiEndian>(table_bytes, &col),
                _ => unreachable!(),
            },
            "column {col} not found"
        );
    }
}

// Based on Bdat::getMember (XC2/DE)
fn find_col_def<E: ByteOrder>(table: &[u8], name: &str) -> bool {
    let hash = LegacyHashTable::new(61).hash(name) as usize;
    let hash_table = &table[E::read_u16(&table[10..]) as usize..];

    let mut visited = HashSet::new();
    let mut slot = &hash_table[hash * 2..hash * 2 + 2];

    // Each slot in the hash table points to a column definition (or 0 if the slot is empty)
    // In the case of a hash collision, the column pointed by the slot has a pointer to the next
    // column with the same hash.
    //
    // Each column definition is 6 bytes long, laid out as follows:
    // |0| column info pointer |2| next node pointer (0 if end) |4| name pointer |6|
    // -|-       2 bytes       -|-           2 bytes            -|-    2 bytes   -|-
    while E::read_u16(slot) != 0 {
        let ptr = E::read_u16(slot) as usize;
        if !visited.insert(ptr) {
            panic!("linked node cycle");
        }
        let name_ptr = E::read_u16(&table[ptr + 4..ptr + 6]) as usize;
        if name_ptr != 0 {
            let read = CStr::from_bytes_until_nul(&table[name_ptr..])
                .expect("no string terminator")
                .to_str()
                .unwrap();
            if read == name {
                return true;
            }
        }
        // Follow linked nodes until name is found
        slot = &table[ptr + 2..ptr + 4];
    }
    false
}
