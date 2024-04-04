//! Tests the hash table found in legacy BDAT files.

use bdat::legacy::{
    LegacyColumnBuilder, LegacyHashTable, LegacyTable, LegacyTableBuilder, LegacyWriteOptions,
};
use bdat::{LegacyFlag, LegacyVersion, SwitchEndian, ValueType, WiiEndian};
use byteorder::ByteOrder;
use std::collections::HashSet;
use std::ffi::CStr;
use std::num::NonZeroUsize;

#[test]
fn hash_table_legacy() {
    for slots in [1, 5, 10, 32, 61, 128] {
        test_table(&create_table(), LegacyVersion::Switch, slots);
    }
}

#[test]
fn hash_table_xcx() {
    for slots in [1, 5, 10, 32, 61, 128] {
        test_table(&create_table(), LegacyVersion::X, slots);
    }
}

// There is no need to adapt/test the hash table on wii, because it is already used (and tested)
// when reading

fn create_table<'b>() -> LegacyTable<'b> {
    LegacyTableBuilder::with_name("Table1")
        .add_column(LegacyColumnBuilder::new(
            ValueType::SignedByte,
            "ColumnXX1".to_string().into(),
        ))
        .add_column(LegacyColumnBuilder::new(
            ValueType::SignedByte,
            "ColumnXX2".to_string().into(),
        ))
        .add_column(LegacyColumnBuilder::new(
            ValueType::String,
            "TotallyDifferentColumn".to_string().into(),
        ))
        .add_column(
            LegacyColumnBuilder::new(ValueType::SignedByte, "ColumnXXFlags".to_string().into())
                .set_flags(vec![
                    LegacyFlag::new_bit("Bit1", 0),
                    LegacyFlag::new_bit("Bit2", 1),
                    LegacyFlag::new_bit("ColumnXX4", 2),
                ])
                .build(),
        )
        .build()
}

fn test_table(table: &LegacyTable, version: LegacyVersion, slots: impl TryInto<NonZeroUsize>) {
    let Ok(slots) = slots.try_into() else {
        panic!("slots = 0")
    };
    let written = match version {
        LegacyVersion::Switch | LegacyVersion::New3ds => {
            bdat::legacy::to_vec_options::<SwitchEndian>(
                [table],
                version,
                LegacyWriteOptions::new().hash_slots(slots),
            )
            .unwrap()
        }
        LegacyVersion::X | LegacyVersion::Wii => bdat::legacy::to_vec_options::<WiiEndian>(
            [table],
            version,
            LegacyWriteOptions::new().hash_slots(slots),
        )
        .unwrap(),
    };

    let table_bytes = &written[12..];

    for col in table.columns().map(|c| c.label().to_string()).chain(
        table
            .columns()
            .flat_map(|c| c.flags())
            .map(|f| f.label().to_string()),
    ) {
        assert!(
            match version {
                LegacyVersion::Switch | LegacyVersion::New3ds =>
                    find_col_def::<SwitchEndian>(table_bytes, &col, usize::from(slots) as u32),
                LegacyVersion::X | LegacyVersion::Wii =>
                    find_col_def::<WiiEndian>(table_bytes, &col, usize::from(slots) as u32),
            },
            "column {col} not found"
        );
    }
}

// Based on Bdat::getMember (XC2/DE)
fn find_col_def<E: ByteOrder>(table: &[u8], name: &str, slots: u32) -> bool {
    let hash = LegacyHashTable::new(slots).hash(name) as usize;
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
