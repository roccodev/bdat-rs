use bdat::{label_hash, BdatFile, BdatVersion, Label, SwitchEndian};

type FileEndian = SwitchEndian;

static TEST_FILE_1: &[u8] = include_bytes!("res/test_modern_1.bdat");

mod common;

#[test]
fn version_detect() {
    assert_eq!(
        BdatVersion::Modern,
        bdat::detect_bytes_version(TEST_FILE_1).unwrap()
    );
}

#[test]
fn basic_read() {
    let tables = bdat::modern::from_bytes::<FileEndian>(TEST_FILE_1)
        .unwrap()
        .get_tables()
        .unwrap();
    assert_eq!(1, tables.len());

    let table = &tables[0];
    assert_eq!(&label_hash!("Table1"), table.name());
    assert_eq!(4, table.column_count());

    let data_t1 = [
        (36_u32, 2.0_f32, "Row 1", label_hash!("Row 1")),
        (2147583648, 0.0000125, "Row 2", label_hash!("Row 2")),
        (3, 104350.27, "Row 3", label_hash!("Row 3")),
        (36, 2.0, "Row 4", label_hash!("Row 4")),
    ];

    for (row, data) in table.rows().zip(data_t1.into_iter()) {
        let mut cells = row.values();
        let a = cells.next().unwrap().to_integer();
        let b = cells.next().unwrap().to_float();
        let c = cells.next().unwrap().as_str();
        let d = cells.next().unwrap().to_integer();
        let row_data = (a, b, c, Label::Hash(d));
        assert_eq!(row_data, data);
    }
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

#[test]
fn table_map() {
    let tables = bdat::modern::from_bytes::<FileEndian>(TEST_FILE_1)
        .unwrap()
        .get_tables_by_name()
        .unwrap();

    assert_eq!(1, tables.len());
    let table = &tables[&label_hash!("Table1")];

    assert_eq!(&label_hash!("Table1"), table.name());
    assert_eq!(None, tables.get(&Label::from("Table2")));

    // Lifetime test
    assert_ne!(0, table.column_count());
}
