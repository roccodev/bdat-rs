use byteorder::{ByteOrder, WriteBytesExt};
use std::collections::HashMap;
use std::io::Write;
use std::marker::PhantomData;
use std::rc::Rc;

use crate::error::Result;
use crate::Table;

struct FileWriter {}

struct TableWriter<'t, E, W> {
    table: Table<'t>,
    buf: W,
    names: StringTable,
    strings: StringTable,
    _endianness: PhantomData<E>,
}

struct CellWriter {}

struct StringTable {
    table: Vec<Rc<str>>,
    offsets: HashMap<Rc<str>, usize>,
    len: usize,
}

impl<'t, E: ByteOrder, W: Write> TableWriter<'t, E, W> {
    fn make_layout(&mut self) -> Result<()> {
        // Table name is the first name
        self.names.get_offset(
            self.table
                .name()
                .expect("no name in legacy table")
                .to_string_convert()
                .as_ref(),
        );

        Ok(())
    }

    fn build_columns(&self) -> Result<Vec<u8>> {
        Ok(vec![])
    }

    fn write_header(&mut self) -> Result<()> {
        self.buf.write_u32::<E>(0x54_41_44_42)?; // "BDAT"
        self.buf.write_u16::<E>(0)?; // Scramble type

        /*
        let offset_names = reader.read_u16::<E>()? as usize;
        let row_len = reader.read_u16::<E>()? as usize;
        let offset_hashes = reader.read_u16::<E>()? as usize;
        let hashes_len = reader.read_u16::<E>()? as usize;
        let offset_rows = reader.read_u16::<E>()? as usize;
        let row_count = reader.read_u16::<E>()? as usize;
        let base_id = reader.read_u16::<E>()? as usize;
        reader.read_u16::<E>()?;
        let scramble_key = reader.read_u16::<E>()?;
        let offset_strings = reader.read_u32::<E>()? as usize;
        let strings_len = reader.read_u32::<E>()? as usize;
        let offset_columns = reader.read_u16::<E>()? as usize;
        let column_count = reader.read_u16::<E>()? as usize;
         */

        Ok(())
    }
}

impl StringTable {
    fn get_offset(&mut self, text: &str) -> usize {
        if let Some(offset) = self.offsets.get(text) {
            return *offset;
        }
        let len = text.len();
        let text: Rc<str> = Rc::from(text);
        let offset = self.len;
        self.len += len;
        self.table.push(text.clone());
        self.offsets.insert(text, offset);
        offset
    }
}
