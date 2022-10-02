# bdat-rs
A library to (de)serialize [Monolithsoft](https://www.monolithsoft.co.jp/)'s proprietary BDAT format, used for data tables in all Xenoblade games.

## Usage
```rs
use serde::Deserialize;
use std::fs::File;
use bdat::BdatFile;
use bdat::Table;
use bdat::MappedTable;

// Given this row definition...
#[derive(Deserialize)]
struct MyRow {
    field_1: i32,
    field_2: i16
}

let file: BdatFile = File::open("myfile.bdat").unwrap().into();

// ...you can parse the table (slower, recommended for tools)
let table: Table<MyRow> = file.get_table();
// or
let table: Table<MyRow> = file.get_table_by_name("MyRow");
s
// ...or you can map the buffer directly (faster, recommended for games, but field order matters)
let table: MappedTable<MyRow> = file.map_table();
// or
let table: MappedTable<MyRow> = file.map_table_by_name("MyRow");

// then access data from a specific row -- BDAT rows start at 1!
println!("{}", table[1].field_1);
```