use std::borrow::Cow;
use std::{
    convert::TryFrom,
    io::{Cursor, Read, Seek, SeekFrom},
    marker::PhantomData,
    num::NonZeroU32,
};

use byteorder::{ByteOrder, ReadBytesExt};

use crate::legacy::float::BdatReal;
use crate::{
    error::{BdatError, Result, Scope},
    types::{Cell, ColumnDef, Label, Row, Table, Value, ValueType},
    TableBuilder,
};

use super::{BdatVersion, FileHeader};

const LEN_COLUMN_DEF_V2: usize = 3;
const LEN_HASH_DEF_V2: usize = 8;

pub struct FileReader<R, E> {
    tables: TableReader<R, E>,
    header: FileHeader,
    version: BdatVersion,
    _endianness: PhantomData<E>,
}

pub struct BdatReader<R, E> {
    stream: R,
    table_offset: usize,
    _endianness: PhantomData<E>,
}

#[derive(Clone)]
pub struct BdatSlice<'b, E> {
    data: Cursor<&'b [u8]>,
    table_offset: usize,
    _endianness: PhantomData<E>,
}

struct TableData<'r> {
    data: Cow<'r, [u8]>,
    string_table_offset: usize,
}

pub trait BdatRead<'b> {
    /// Read a single 32-bit unsigned integer at the current position.
    fn read_u32(&mut self) -> Result<u32>;

    /// Get a slice (or buffer) to the full binary stream for a single table.
    fn read_table_data(&mut self, length: usize) -> Result<Cow<'b, [u8]>>;

    /// Seek the current position to the next table at the given offset.
    fn seek_table(&mut self, offset: usize) -> Result<()>;
}

struct HeaderReader<R, E> {
    reader: R,
    _endianness: PhantomData<E>,
}

struct TableReader<R, E> {
    reader: R,
    _endianness: PhantomData<E>,
}

impl<'b, R, E> FileReader<R, E>
where
    R: BdatRead<'b>,
    E: ByteOrder,
{
    pub(super) fn read_file(mut reader: R) -> Result<Self> {
        if reader.read_u32()? == 0x54_41_44_42 {
            if reader.read_u32()? != 0x01_00_10_04 {
                return Err(BdatError::MalformedBdat(Scope::File));
            }
            Self::new_with_header(reader, BdatVersion::Modern)
        } else {
            Self::new_with_header(reader, BdatVersion::Legacy)
        }
    }

    /// Reads all tables from the BDAT source.
    pub fn get_tables(&mut self) -> Result<Vec<Table<'b>>> {
        let mut tables = Vec::with_capacity(self.header.table_count);

        for i in 0..self.header.table_count {
            self.tables
                .reader
                .seek_table(self.header.table_offsets[i])?;
            let table = self.read_table()?;
            tables.push(table);
        }

        Ok(tables)
    }

    /// Returns the number of tables in the BDAT file.
    pub fn table_count(&self) -> usize {
        self.header.table_count
    }

    fn read_table(&mut self) -> Result<Table<'b>> {
        match self.version {
            BdatVersion::Modern => self.tables.read_table_v2(),
            _ => todo!("legacy bdats"),
        }
    }

    fn new_with_header(reader: R, version: BdatVersion) -> Result<Self> {
        let mut header_reader = HeaderReader::<R, E>::new(reader);
        let header = header_reader.read_header(version)?;
        Ok(Self {
            tables: TableReader::new(header_reader.reader),
            header,
            version,
            _endianness: PhantomData,
        })
    }
}

impl<'b, E> BdatSlice<'b, E> {
    pub fn new(bytes: &'b [u8]) -> Self {
        Self {
            data: Cursor::new(bytes),
            table_offset: 0,
            _endianness: PhantomData,
        }
    }
}

impl<R, E> BdatReader<R, E> {
    pub fn new(reader: R) -> Self {
        Self {
            stream: reader,
            table_offset: 0,
            _endianness: PhantomData,
        }
    }
}

impl<'b, R: BdatRead<'b>, E: ByteOrder> HeaderReader<R, E> {
    fn new(reader: R) -> Self {
        Self {
            reader,
            _endianness: PhantomData,
        }
    }

    fn read_header(&mut self, version: BdatVersion) -> Result<FileHeader> {
        let table_count = self.reader.read_u32()? as usize;
        let mut table_offsets = Vec::with_capacity(table_count);

        if version == BdatVersion::Modern {
            self.reader.read_u32()?; // File size
        }

        for _ in 0..table_count {
            table_offsets.push(self.reader.read_u32()? as usize);
        }

        Ok(FileHeader {
            table_count,
            table_offsets,
        })
    }
}

impl<'b, R: BdatRead<'b>, E: ByteOrder> TableReader<R, E> {
    fn new(reader: R) -> Self {
        Self {
            reader,
            _endianness: PhantomData,
        }
    }

    fn read_table_v2(&mut self) -> Result<Table<'b>> {
        if self.reader.read_u32()? != 0x54_41_44_42 || self.reader.read_u32()? != 0x3004 {
            return Err(BdatError::MalformedBdat(Scope::Table));
        }

        let columns = self.reader.read_u32()? as usize;
        let rows = self.reader.read_u32()? as usize;
        let base_id = self.reader.read_u32()? as usize;
        if self.reader.read_u32()? != 0 {
            panic!("Found unknown value at index 0x14 that was not 0");
        }

        let offset_col = self.reader.read_u32()? as usize;
        let offset_hash = self.reader.read_u32()? as usize;
        let offset_row = self.reader.read_u32()? as usize;
        #[allow(clippy::needless_late_init)]
        let offset_string;

        let row_length = self.reader.read_u32()? as usize;
        offset_string = self.reader.read_u32()? as usize;
        let str_length = self.reader.read_u32()? as usize;

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
        let table_raw = self.reader.read_table_data(*table_len)?;
        let table_data = TableData::new(table_raw, offset_string);

        let name = table_data.get_name::<E>()?.map(|h| Label::Hash(h.into()));
        let mut col_data = Vec::with_capacity(columns);
        let mut row_data = Vec::with_capacity(rows);

        let mut data_offset = 0;
        for i in 0..columns {
            let col = &table_data.data[offset_col + i * LEN_COLUMN_DEF_V2..];
            let ty = ValueType::try_from(col[0]).expect("unsupported value type");
            let name_offset = (&col[1..]).read_u16::<E>()?;
            let label = table_data.get_label::<E>(name_offset as usize)?;

            col_data.push(ColumnDef {
                value_type: ty,
                label,
                offset: data_offset,
                flags: Vec::new(),
            });
            data_offset += ty.data_len();
        }

        for i in 0..rows {
            let row = &table_data.data[offset_row + i * row_length..];
            let mut cells = Vec::with_capacity(col_data.len());
            let mut cursor = Cursor::new(row);
            for col in &col_data {
                let value = Self::read_value_v2(&table_data, &mut cursor, col.value_type)?;
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
        table_data: &TableData<'b>,
        mut buf: impl Read,
        col_type: ValueType,
    ) -> Result<Value<'b>> {
        Ok(match col_type {
            ValueType::Unknown => Value::Unknown,
            ValueType::UnsignedByte => Value::UnsignedByte(buf.read_u8()?),
            ValueType::UnsignedShort => Value::UnsignedShort(buf.read_u16::<E>()?),
            ValueType::UnsignedInt => Value::UnsignedInt(buf.read_u32::<E>()?),
            ValueType::SignedByte => Value::SignedByte(buf.read_i8()?),
            ValueType::SignedShort => Value::SignedShort(buf.read_i16::<E>()?),
            ValueType::SignedInt => Value::SignedInt(buf.read_i32::<E>()?),
            ValueType::String => {
                Value::String(table_data.get_string(buf.read_u32::<E>()? as usize, usize::MAX)?)
            }
            ValueType::Float => Value::Float(BdatReal::Floating(buf.read_f32::<E>()?.into())),
            ValueType::Percent => Value::Percent(buf.read_u8()?),
            ValueType::HashRef => Value::HashRef(buf.read_u32::<E>()?),
            ValueType::DebugString => Value::DebugString(
                table_data.get_string(buf.read_u32::<E>()? as usize, usize::MAX)?,
            ),
            ValueType::Unknown2 => Value::Unknown2(buf.read_u8()?),
            ValueType::Unknown3 => Value::Unknown3(buf.read_u16::<E>()?),
        })
    }
}

impl<'r> TableData<'r> {
    fn new(data: Cow<'r, [u8]>, strings_offset: usize) -> TableData<'r> {
        Self {
            data,
            string_table_offset: strings_offset,
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
    fn get_string(&self, offset: usize, limit: usize) -> Result<Cow<'r, str>> {
        let str_ptr = self.string_table_offset + offset;
        let len = self.data[str_ptr..]
            .split(|&b| b == 0)
            .take(1)
            .flatten()
            .take(limit)
            .count();
        let str = match &self.data {
            Cow::Borrowed(data) => {
                Cow::Borrowed(std::str::from_utf8(&data[str_ptr..str_ptr + len])?)
            }
            Cow::Owned(data) => {
                Cow::Owned(std::str::from_utf8(&data[str_ptr..str_ptr + len])?.to_string())
            }
        };
        Ok(str)
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

impl<'b, E> BdatRead<'b> for BdatSlice<'b, E>
where
    E: ByteOrder,
{
    fn read_table_data(&mut self, length: usize) -> Result<Cow<'b, [u8]>> {
        Ok(Cow::Borrowed(
            &self.data.clone().into_inner()[self.table_offset..self.table_offset + length],
        ))
    }

    #[inline]
    fn read_u32(&mut self) -> Result<u32> {
        Ok(self.data.read_u32::<E>()?)
    }

    fn seek_table(&mut self, offset: usize) -> Result<()> {
        self.data.seek(SeekFrom::Start(offset as u64))?;
        self.table_offset = offset;
        Ok(())
    }
}

impl<'b, R, E> BdatRead<'b> for BdatReader<R, E>
where
    R: Read + Seek,
    E: ByteOrder,
{
    fn read_table_data(&mut self, length: usize) -> Result<Cow<'b, [u8]>> {
        let mut table_raw = vec![0u8; length];
        self.stream
            .seek(SeekFrom::Start(self.table_offset as u64))?;
        self.stream.read_exact(&mut table_raw)?;
        Ok(table_raw.into())
    }

    #[inline]
    fn read_u32(&mut self) -> Result<u32> {
        Ok(self.stream.read_u32::<E>()?)
    }

    fn seek_table(&mut self, offset: usize) -> Result<()> {
        self.stream.seek(SeekFrom::Start(offset as u64))?;
        self.table_offset = offset;
        Ok(())
    }
}
