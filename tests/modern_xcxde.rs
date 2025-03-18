use bdat::{hash::murmur3_str, label_hash, BdatFile, BdatVersion, SwitchEndian};

type FileEndian = SwitchEndian;

static TEST_FILE_1: &[u8] = include_bytes!("res/test_modern_xde.bdat");

#[test]
fn version_detect() {
    assert_eq!(
        BdatVersion::Modern,
        bdat::detect_bytes_version(TEST_FILE_1).unwrap()
    );
}

#[test]
fn hash_table() {
    let tables = bdat::modern::from_bytes::<FileEndian>(TEST_FILE_1)
        .unwrap()
        .get_tables()
        .unwrap();
    assert_eq!(1, tables.len());

    let table = &tables[0];
    assert_eq!(&label_hash!("Table1"), table.name());
    assert_eq!(4, table.column_count());

    /* [
        (36_u32, 2.0_f32, "Row 1", label_hash!("Row 1")),
        (2147583648, 0.0000125, "Row 2", label_hash!("Row 2")),
        (3, 104350.27, "Row 3", label_hash!("Row 2")), <-- duplicate
        (36, 2.0, "Row 4", label_hash!("Row 4")),
    ] */

    // Non-duplicate rows
    assert_eq!(
        table
            .row_by_hash(murmur3_str("Row 1"))
            .get(label_hash!("Col1"))
            .to_integer(),
        36
    );
    assert_eq!(
        table
            .row_by_hash(murmur3_str("Row 4"))
            .get(label_hash!("Col3"))
            .as_str(),
        "Row 4"
    );
    // Duplicate row, should always match the first row
    assert_eq!(
        table
            .row_by_hash(murmur3_str("Row 2"))
            .get(label_hash!("Col1"))
            .to_integer(),
        2147583648
    );
}

#[test]
fn write_back() {
    let tables = bdat::modern::from_bytes::<FileEndian>(TEST_FILE_1)
        .unwrap()
        .get_tables()
        .unwrap();
    let mut new_out = bdat::modern::to_vec::<FileEndian>(&tables).unwrap();
    let new_tables = bdat::modern::from_bytes::<FileEndian>(&mut new_out)
        .unwrap()
        .get_tables()
        .unwrap();
    assert_eq!(tables, new_tables);
}
