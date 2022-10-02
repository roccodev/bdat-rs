use std::{
    io::{BufRead, Read},
    marker::PhantomData,
    num::NonZeroU32,
};

use byteorder::{ByteOrder, LittleEndian, ReadBytesExt};

use crate::{
    error::{BdatError, Result, Scope},
    types::{ColumnDef, Label, RawTable, Value},
};

const LEN_COLUMN_DEF_V2: usize = 3;
const LEN_HASH_DEF_V2: usize = 8;

struct BdatReader<R, E = LittleEndian> {
    stream: R,
    _endianness: PhantomData<E>,
}

struct TableData<'r> {
    data: &'r [u8],
    string_table_offset: usize,
    hash_table_offset: usize,
    columns_offset: usize,
    rows_offset: usize,
}

impl<R, E> BdatReader<R, E>
where
    R: Read,
    E: ByteOrder,
{
    fn read_table_v2(&mut self) -> Result<RawTable> {
        if self.r_i32()? != 0x42_44_41_54 || self.r_i32()? != 0x3004 {
            return Err(BdatError::MalformedBdat(Scope::Table));
        }

        let columns = self.r_i32()? as usize;
        let rows = self.r_i32()? as usize;
        let base_id = self.r_i32()?;
        self.r_i32()?;

        let offset_col = self.r_i32()? as usize;
        let offset_hash = self.r_i32()? as usize;
        let offset_row = self.r_i32()? as usize;
        let offset_string;

        let row_length = self.r_i32()? as usize;
        offset_string = self.r_i32()? as usize;
        let str_length = self.r_i32()? as usize;

        let lengths = [
            offset_col + LEN_COLUMN_DEF_V2 * columns,
            offset_hash + LEN_HASH_DEF_V2 * rows,
            offset_row + row_length * rows,
            offset_string + str_length,
        ];
        let table_len = lengths.iter().max_by_key(|&i| i).expect("todo");
        let table_raw = vec![0u8; *table_len];
        let table_data = TableData::read(
            &table_raw,
            offset_string,
            offset_hash,
            offset_col,
            offset_row,
        );

        let name = table_data.get_name::<E>()?.map(|h| Label::Hash(h.into()));
        let mut col_data = Vec::with_capacity(columns);
        let mut row_data = Vec::with_capacity(rows);

        for i in 0..columns {
            let col = &table_raw[offset_col + i * LEN_COLUMN_DEF_V2..];
            let ty = col[0];
            let name_offset = (&col[1..]).read_u16::<E>()?;
            let label = table_data.get_label::<E>(name_offset as usize)?;

            col_data.push(ColumnDef { ty, label });
        }

        for i in 0..rows {
            let row = &table_raw[offset_row + i * row_length..];
            // TODO parse values
        }

        Ok(RawTable {
            name,
            columns: col_data,
            rows: row_data,
        })
    }

    #[inline]
    fn r_i32(&mut self) -> Result<i32> {
        Ok(self.stream.read_i32::<E>().expect("todo"))
    }
}

impl<'r> TableData<'r> {
    fn read(
        data: &'r [u8],
        strings_offset: usize,
        hashes_offset: usize,
        columns_offset: usize,
        rows_offset: usize,
    ) -> TableData<'r> {
        Self {
            data,
            string_table_offset: strings_offset,
            hash_table_offset: hashes_offset,
            columns_offset,
            rows_offset,
        }
    }

    /// Returns the table's hashed name, or [`None`] if it could not be found.
    fn get_name<E>(&self) -> Result<Option<NonZeroU32>>
    where
        E: ByteOrder,
    {
        let id = (&self.data[self.string_table_offset + 1..]).read_u32::<E>()?;
        Ok(NonZeroU32::new(id))
    }

    /// Reads a null-terminated UTF-8 encoded string from the string table at the given offset
    fn get_string(&'r self, offset: usize, limit: usize) -> Result<&'r str> {
        let str_ptr = self.string_table_offset + offset;
        let len = self.data[str_ptr..]
            .split(|&b| b == 0)
            .take(1)
            .flatten()
            .take(limit)
            .count();
        Ok(std::str::from_utf8(&self.data[str_ptr..len])?)
    }

    /// Reads a column label (either a string or a hash) from the string table at the given offset
    fn get_label<E>(&self, offset: usize) -> Result<Label>
    where
        E: ByteOrder,
    {
        if self.are_labels_hashed() {
            Ok(Label::String(
                self.get_string(offset, usize::MAX)?.to_string(),
            ))
        } else {
            Ok(Label::Hash(
                (&self.data[self.string_table_offset + offset..]).read_u32::<E>()?,
            ))
        }
    }

    fn are_labels_hashed(&self) -> bool {
        self.data[self.string_table_offset] != 0
    }
}
