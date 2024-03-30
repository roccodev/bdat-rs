use bdat::legacy::LegacyWriteOptions;
use bdat::{BdatFile, Cell, Label, LegacyVersion, SwitchEndian, Value};

type FileEndian = SwitchEndian;

static TEST_FILE_1: &[u8] = include_bytes!("res/test_legacy_1.bdat");

mod common;

#[test]
fn version_detect() {
    assert_eq!(
        bdat::detect_bytes_version(TEST_FILE_1).unwrap(),
        LegacyVersion::Switch.into(),
    );
}

#[test]
fn basic_read() {
    let tables = bdat::legacy::from_bytes_copy::<FileEndian>(TEST_FILE_1, LegacyVersion::Switch)
        .unwrap()
        .get_tables()
        .unwrap();
    assert_eq!(1, tables.len());

    let table = &tables[0];
    assert_eq!("Table1", table.name());
    assert_eq!(4, table.column_count());

    let flags_col = table
        .columns()
        .find(|c| c.label() == "value_flags")
        .unwrap();
    assert_eq!(3, flags_col.flags().len());

    let data_t1 = [
        (
            36_u32,
            2.0_f32,
            vec!["Row 1a", "Row 1bb", "Row 1ccc"],
            vec![1u32, 3, 1],
        ),
        (
            2147583648,
            0.0000125,
            vec!["Row 2a", "Row 2bb", "Row 2ccc"],
            vec![0, 2, 1],
        ),
        (
            3,
            104350.27,
            vec!["Row 3a", "Row 3bb", "Row 3ccc"],
            vec![1, 1, 0],
        ),
        (
            36,
            2.0,
            vec!["Row 4a", "Row 4bb", "Row 4ccc"],
            vec![0, 0, 0],
        ),
    ];

    for (row, data) in table.rows().zip(data_t1.into_iter()) {
        let mut cells = row.cells();
        let a = cells
            .next()
            .unwrap()
            .as_single()
            .unwrap()
            .clone()
            .to_integer();
        let b = cells
            .next()
            .unwrap()
            .as_single()
            .unwrap()
            .clone()
            .to_float();
        let c = match cells.next().unwrap() {
            Cell::List(l) => l
                .iter()
                .map(|v| match v {
                    Value::String(s) => s.as_ref(),
                    _ => panic!(),
                })
                .collect::<Vec<_>>(),
            _ => panic!(),
        };
        let d = match cells.next().unwrap() {
            Cell::Flags(flags) => flags.clone(),
            _ => panic!(),
        };
        let row_data = (a, b, c, d);
        assert_eq!(row_data, data);
    }
}

#[test]
fn write_back() {
    let tables = bdat::legacy::from_bytes_copy::<FileEndian>(TEST_FILE_1, LegacyVersion::Switch)
        .unwrap()
        .get_tables()
        .unwrap();
    let mut new_out = bdat::legacy::to_vec_options::<FileEndian>(
        &tables,
        LegacyVersion::Switch,
        LegacyWriteOptions::new().scramble(true),
    )
    .unwrap();
    let new_tables = bdat::legacy::from_bytes::<FileEndian>(&mut new_out, LegacyVersion::Switch)
        .unwrap()
        .get_tables()
        .unwrap();
    assert_eq!(tables, new_tables);
}

#[test]
fn duplicate_columns() {
    let tables = [common::duplicate_table_create()];

    let mut bytes = bdat::legacy::to_vec::<FileEndian>(&tables, LegacyVersion::Switch).unwrap();
    let back = bdat::legacy::from_bytes::<FileEndian>(&mut bytes, LegacyVersion::Switch)
        .unwrap()
        .get_tables()
        .unwrap();

    assert_eq!(tables[0], back[0]);
}

#[test]
fn table_map() {
    let tables = bdat::legacy::from_bytes_copy::<FileEndian>(TEST_FILE_1, LegacyVersion::Switch)
        .unwrap()
        .get_tables_by_name()
        .unwrap();

    assert_eq!(1, tables.len());
    let table = &tables[&Label::from("Table1")];

    assert_eq!("Table1", table.name());
    assert_eq!(None, tables.get(&Label::from("Table2")));

    // Lifetime test
    assert_ne!(0, table.column_count());
}
