use std::{
    convert::TryFrom,
    io::{Cursor, Read, Seek, SeekFrom},
    marker::PhantomData,
    num::NonZeroU32,
};

use byteorder::{ByteOrder, ReadBytesExt};

use crate::{
    error::{BdatError, Result, Scope},
    types::{Cell, ColumnDef, Label, RawTable, Row, Value, ValueType},
    TableBuilder,
};

use super::{BdatVersion, FileHeader};

const LEN_COLUMN_DEF_V2: usize = 3;
const LEN_HASH_DEF_V2: usize = 8;

pub(crate) struct BdatReader<R, E> {
    stream: R,
    version: BdatVersion,
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
    R: Read + Seek,
    E: ByteOrder,
{
    pub fn new(stream: R, version: BdatVersion) -> Self {
        Self {
            stream,
            version,
            _endianness: PhantomData,
        }
    }

    pub fn read_file(mut stream: R) -> Result<Self> {
        if stream.read_u32::<E>()? == 0x54_41_44_42 {
            if stream.read_u32::<E>()? != 0x01_00_10_04 {
                return Err(BdatError::MalformedBdat(Scope::File));
            }
            Ok(Self::new(stream, BdatVersion::Modern))
        } else {
            Ok(Self::new(stream, BdatVersion::Legacy))
        }
    }

    pub fn read_table(&mut self) -> Result<RawTable> {
        match self.version {
            BdatVersion::Legacy => todo!("legacy bdats"),
            BdatVersion::Modern => self.read_table_v2(),
        }
    }

    fn read_table_v2(&mut self) -> Result<RawTable> {
        let base_offset = self.stream.stream_position()?;

        if self.r_u32()? != 0x54_41_44_42 || self.r_u32()? != 0x3004 {
            return Err(BdatError::MalformedBdat(Scope::Table));
        }

        let columns = self.r_u32()? as usize;
        let rows = self.r_u32()? as usize;
        let base_id = self.r_u32()? as usize;
        if self.r_u32()? != 0 {
            panic!("Found unknown value at index 0x14 that was not 0");
        }

        let offset_col = self.r_u32()? as usize;
        let offset_hash = self.r_u32()? as usize;
        let offset_row = self.r_u32()? as usize;
        #[allow(clippy::needless_late_init)]
        let offset_string;

        let row_length = self.r_u32()? as usize;
        offset_string = self.r_u32()? as usize;
        let str_length = self.r_u32()? as usize;

        let lengths = [
            offset_col + LEN_COLUMN_DEF_V2 * columns,
            offset_hash + LEN_HASH_DEF_V2 * rows,
            offset_row + row_length * rows,
            offset_string + str_length,
        ];
        let table_len = lengths
            .iter()
            .max_by_key(|&i| i)
            .expect("could not determine table length");
        let mut table_raw = vec![0u8; *table_len];
        self.stream.seek(SeekFrom::Start(base_offset))?;
        self.stream.read_exact(&mut table_raw)?;
        let table_data = TableData::new(
            &table_raw,
            offset_string,
            offset_hash,
            offset_col,
            offset_row,
        );

        let name = table_data.get_name::<E>()?.map(|h| Label::Hash(h.into()));
        let mut col_data = Vec::with_capacity(columns);
        let mut row_data = Vec::with_capacity(rows);

        let mut data_offset = 0;
        for i in 0..columns {
            let col = &table_raw[offset_col + i * LEN_COLUMN_DEF_V2..];
            let ty = ValueType::try_from(col[0]).expect("unsupported value type");
            let name_offset = (&col[1..]).read_u16::<E>()?;
            let label = table_data.get_label::<E>(name_offset as usize)?;

            col_data.push(ColumnDef {
                ty,
                label,
                offset: data_offset,
            });
            data_offset += ty.data_len();
        }

        for i in 0..rows {
            let row = &table_raw[offset_row + i * row_length..];
            let mut cells = Vec::with_capacity(col_data.len());
            let mut cursor = Cursor::new(row);
            for col in &col_data {
                let value = Self::read_value_v2(&table_data, &mut cursor, col.ty)?;
                cells.push(Cell::Single(value));
            }
            row_data.push(Row {
                id: base_id + i,
                cells,
            });
        }

        Ok(TableBuilder::new()
            .set_name(name)
            .set_columns(col_data)
            .set_rows(row_data)
            .build())
    }

    fn read_value_v2(
        table_data: &TableData,
        mut buf: impl Read,
        col_type: ValueType,
    ) -> Result<Value> {
        Ok(match col_type {
            ValueType::Unknown => Value::Unknown,
            ValueType::UnsignedByte => Value::UnsignedByte(buf.read_u8()?),
            ValueType::UnsignedShort => Value::UnsignedShort(buf.read_u16::<E>()?),
            ValueType::UnsignedInt => Value::UnsignedInt(buf.read_u32::<E>()?),
            ValueType::SignedByte => Value::SignedByte(buf.read_i8()?),
            ValueType::SignedShort => Value::SignedShort(buf.read_i16::<E>()?),
            ValueType::SignedInt => Value::SignedInt(buf.read_i32::<E>()?),
            ValueType::String => Value::String(
                table_data
                    .get_string(buf.read_u32::<E>()? as usize, usize::MAX)?
                    .to_string(),
            ),
            ValueType::Float => Value::Float(buf.read_f32::<E>()?),
            ValueType::Percent => Value::Percent(buf.read_u8()?),
            ValueType::HashRef => Value::HashRef(buf.read_u32::<E>()?),
            ValueType::Unknown1 => Value::Unknown1(
                table_data
                    .get_string(buf.read_u32::<E>()? as usize, usize::MAX)?
                    .to_string(),
            ),
            ValueType::Unknown2 => Value::Unknown2(buf.read_u8()?),
            ValueType::Unknown3 => Value::Unknown3(buf.read_u16::<E>()?),
        })
    }

    pub(super) fn read_header(&mut self) -> Result<FileHeader> {
        let table_count = self.r_u32()? as usize;
        let mut table_offsets = Vec::with_capacity(table_count);

        if self.version == BdatVersion::Modern {
            self.stream.read_u32::<E>()?; // File size
        }

        for _ in 0..table_count {
            table_offsets.push(self.r_u32()? as usize);
        }

        Ok(FileHeader {
            table_count,
            table_offsets,
        })
    }

    pub(super) fn stream_mut(&mut self) -> &mut R {
        &mut self.stream
    }

    #[inline]
    fn r_u32(&mut self) -> Result<u32> {
        Ok(self.stream.read_u32::<E>()?)
    }
}

impl<'r> TableData<'r> {
    fn new(
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
        Ok(std::str::from_utf8(&self.data[str_ptr..str_ptr + len])?)
    }

    /// Reads a column label (either a string or a hash) from the string table at the given offset
    fn get_label<E>(&self, offset: usize) -> Result<Label>
    where
        E: ByteOrder,
    {
        if self.are_labels_hashed() {
            Ok(Label::Hash(
                (&self.data[self.string_table_offset + offset..]).read_u32::<E>()?,
            ))
        } else {
            Ok(Label::String(
                self.get_string(offset, usize::MAX)?.to_string(),
            ))
        }
    }

    fn are_labels_hashed(&self) -> bool {
        self.data[self.string_table_offset] == 0
    }
}
