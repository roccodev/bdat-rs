// Flag notes
//
// BDAT column definition
// - [u32] Info offset
// - [u16] Name
// ------> INFO ------>
// - [u8] Cell type (Flag, Value, Array)
// --------> extra INFO for Flag ------->
// - [u8] Flag Index
// - [u32] Flag Mask
// - [u32] Parent Offset (pointer to Column definition)
// -------> extra INFO for Value ------->
// - [u8] Value Type
// - [u16] Value Offset
// -------> extra INFO for Array ------->
// - [u8] Value Type
// - [u16] Value Offset
// - [u16] Array Size

use std::borrow::Cow;
use std::ffi::CStr;
use std::io::{Cursor, Read, Seek, SeekFrom};
use std::marker::PhantomData;
use std::ops::{Deref, Range, RangeFrom};

use byteorder::{ByteOrder, NativeEndian, ReadBytesExt};

use crate::error::{Result, Scope};
use crate::legacy::float::BdatReal;
use crate::legacy::scramble::{unscramble, ScrambleType};
use crate::{
    BdatError, BdatFile, BdatVersion, Cell, ColumnDef, FlagDef, Label, Row, Table, TableBuilder,
    Value, ValueType,
};

use super::{FileHeader, TableHeader};

const COLUMN_DEF_LEN: usize = 6;
type Utf<'t> = Cow<'t, str>; // TODO: export to use in XC3 bdats

pub struct LegacySlice<'t, E> {
    data: &'t [u8],
    header: FileHeader,
    version: BdatVersion,
    _endianness: PhantomData<E>,
}

pub struct LegacyReader<R, E> {
    reader: R,
    header: FileHeader,
    version: BdatVersion,
    _endianness: PhantomData<E>,
}

struct TableReader<'t, E> {
    header: TableHeader,
    version: BdatVersion,
    data: Cursor<Cow<'t, [u8]>>,
    _endianness: PhantomData<E>,
}

struct ColumnReader<'a, 't, E> {
    table: &'a TableReader<'t, E>,
    data: Cursor<&'a Cow<'t, [u8]>>,
    info_ptr: usize,
    _endianness: PhantomData<E>,
}

struct RowReader<'a, 't: 'a, E> {
    table: &'a mut TableReader<'t, E>,
    /// The cells for the row currently being read
    cells: Vec<Option<Cell<'t>>>,
    columns: &'a [ColumnDef],
    row_idx: usize,
}

#[derive(Debug, Clone)]
struct ColumnData<'t> {
    name: Utf<'t>,
    info_offset: usize,
    cell: ColumnCell,
}

#[derive(Debug, Clone, Copy)]
struct FlagData {
    index: usize,
    mask: u32,
    parent_info_offset: usize,
}

#[derive(Debug, Clone, Copy)]
struct ValueData {
    value_type: ValueType,
    offset: usize,
}

#[derive(Debug, Clone, Copy)]
enum ColumnCell {
    Flag(FlagData), // this is used in the flag's column, not the parent's
    Value(ValueData),
    Array(ValueData, usize), // array size
}

struct Flags<'t>(Vec<ColumnData<'t>>);

impl<R: Read + Seek, E: ByteOrder> LegacyReader<R, E> {
    pub fn new(mut reader: R, version: BdatVersion) -> Result<Self> {
        let header = FileHeader::read::<_, E>(&mut reader)?;
        Ok(Self {
            header,
            version,
            reader,
            _endianness: PhantomData,
        })
    }
}

impl<'t, E: ByteOrder> LegacySlice<'t, E> {
    pub fn new(bytes: &'t mut [u8], version: BdatVersion) -> Result<Self> {
        let header = FileHeader::read::<_, E>(Cursor::new(&bytes))?;
        // TODO
        header.for_each_table_mut(bytes, |table| {
            let header = TableHeader::read::<E>(Cursor::new(&table))?;
            header.unscramble_data(table);
            table[4] = 0;
            Ok::<_, BdatError>(())
        })?;
        Ok(Self {
            header,
            version,
            data: bytes,
            _endianness: PhantomData,
        })
    }
}

impl FileHeader {
    pub fn read<R: Read + Seek, E: ByteOrder>(mut reader: R) -> Result<Self> {
        let table_count = reader.read_u32::<E>()? as usize;
        let file_size = reader.read_u32::<E>()? as usize;
        let mut offsets = Vec::with_capacity(table_count);
        for _ in 0..table_count {
            offsets.push(reader.read_u32::<E>()? as usize);
        }
        Ok(Self {
            table_count,
            file_size,
            table_offsets: offsets,
        })
    }

    pub fn for_each_table_mut<F, E>(&self, data: &mut [u8], mut f: F) -> std::result::Result<(), E>
    where
        F: FnMut(&mut [u8]) -> std::result::Result<(), E>,
    {
        // An iterator for this would require unsafe code because it's returning mutable
        // references

        match self.table_offsets.len() {
            0 => return Ok(()),
            1 => return f(&mut data[self.table_offsets[0]..self.file_size]),
            _ => {}
        }

        for bounds in self.table_offsets.windows(2) {
            match *bounds {
                [s, e] => f(&mut data[s..e])?,
                [s] => f(&mut data[s..self.file_size])?,
                _ => return Ok(()),
            }
        }

        f(&mut data[(self.table_offsets[self.table_offsets.len() - 1])..self.file_size])?;

        Ok(())
    }
}

impl TableHeader {
    pub fn read<E: ByteOrder>(mut reader: impl Read) -> Result<Self> {
        if reader.read_u32::<NativeEndian>()? != 0x54_41_44_42 {
            // BDAT - doesn't change with endianness
            return Err(BdatError::MalformedBdat(Scope::Table));
        }
        let scramble_id = reader.read_u16::<E>()? as usize;
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

        Ok(Self {
            scramble_type: match scramble_id {
                0 => ScrambleType::None,
                768 /* XCX */ | 2 => ScrambleType::Scrambled(scramble_key),
                _ => ScrambleType::Unknown,
            },
            hashes: (offset_hashes, hashes_len).into(),
            strings: (offset_strings, strings_len).into(),
            offset_columns,
            offset_names,
            offset_rows,
            column_count,
            row_count,
            row_len,
            base_id,
        })
    }

    pub fn unscramble_data(&self, data: &mut [u8]) {
        let scramble_key = match self.scramble_type {
            ScrambleType::Scrambled(key) => key,
            _ => return,
        };
        // Unscramble column names and string table
        unscramble(
            &mut data[self.offset_names..self.hashes.offset],
            scramble_key,
        );
        unscramble(&mut data[self.strings.range()], scramble_key);
    }

    fn get_table_len(&self) -> usize {
        [
            self.hashes.max_offset(),
            self.strings.max_offset(),
            self.offset_rows + self.row_len * self.row_count,
            self.offset_columns + COLUMN_DEF_LEN * self.column_count,
        ]
        .into_iter()
        .max()
        .unwrap()
    }
}

impl<'t, E: ByteOrder> TableReader<'t, E> {
    fn from_reader<R: Read + Seek>(mut reader: R, version: BdatVersion) -> Result<Self> {
        let original_pos = reader.stream_position()?;
        let header = TableHeader::read::<E>(&mut reader)?;
        reader.seek(SeekFrom::Start(original_pos))?;

        let table_len = header.get_table_len();
        let mut table_data: Vec<u8> = Vec::with_capacity(table_len);
        let bytes_read = reader
            .take(table_len.try_into().unwrap())
            .read_to_end(&mut table_data)?;
        if bytes_read != table_len {
            todo!("unexpected eof");
        }

        match header.scramble_type {
            ScrambleType::Scrambled(_) => header.unscramble_data(&mut table_data),
            ScrambleType::Unknown => panic!("Unknown scramble type"),
            ScrambleType::None => {}
        };

        Ok(Self {
            header,
            version,
            data: Cursor::new(Cow::Owned(table_data)),
            _endianness: PhantomData,
        })
    }

    fn from_slice(bytes: &'t [u8], version: BdatVersion) -> Result<TableReader<'t, E>> {
        let mut reader = Cursor::new(&bytes);
        let original_pos = reader.stream_position()?;
        let header = TableHeader::read::<E>(&mut reader)?;
        reader.seek(SeekFrom::Start(original_pos))?;

        match header.scramble_type {
            ScrambleType::Scrambled(_) => {} /*header.unscramble_data(bytes)*/
            ScrambleType::Unknown => panic!("Unknown scramble type"),
            ScrambleType::None => {}
        };

        Ok(Self {
            header,
            version,
            data: Cursor::new(Cow::Borrowed(bytes)),
            _endianness: PhantomData,
        })
    }

    fn read(mut self) -> Result<Table<'t>> {
        let name = self.read_name(0)?.to_string(); // TODO
        self.data.seek(SeekFrom::Start(
            self.header.offset_columns.try_into().unwrap(),
        ))?;
        let mut seek = self.data.position();
        let (flags, columns) = (0..self.header.column_count)
            .map(|_| {
                let col = ColumnReader::new(&self, seek)?.read_column()?;
                seek += COLUMN_DEF_LEN as u64;
                Ok(col)
            })
            .partition::<Vec<_>, _>(|c| {
                c.as_ref()
                    .ok()
                    .and_then(|c| c.cell.is_flag().then_some(()))
                    .is_some()
            });
        let (columns_src, flags): (Vec<ColumnData>, Flags) = (
            columns.into_iter().collect::<Result<_>>()?,
            Flags::new(flags.into_iter().collect::<Result<_>>()?),
        );

        let column_cells = columns_src.iter().map(|c| c.cell).collect::<Vec<_>>();

        // De-flag-ify
        let columns = columns_src
            .clone() // TODO
            .into_iter()
            .map(|c| {
                ColumnDef {
                    label: Label::String(c.name.to_string()),
                    value_type: c.cell.value().value_type,
                    offset: c.cell.value().offset,
                    count: match c.cell {
                        ColumnCell::Array(_, c) => c,
                        _ => 1,
                    },
                    // TODO optimize?
                    flags: flags
                        .get_from_parent(c.info_offset)
                        .map(|f| {
                            let ColumnCell::Flag(flag) = &f.cell else { unreachable!() };
                            FlagDef {
                                label: Label::String(f.name.to_string()),
                                flag_index: flag.index,
                                mask: flag.mask,
                            }
                        })
                        .collect(),
                }
            })
            .collect::<Vec<_>>();

        self.data
            .seek(SeekFrom::Start(self.header.offset_rows.try_into().unwrap()))?;

        let mut rows = vec![];
        let row_count = self.header.row_count;
        let base_id = self.header.base_id;
        let mut row_reader = RowReader::new(&mut self, &columns);
        for i in 0..row_count {
            let cells = row_reader.read_row()?;
            rows.push(Row::new(base_id + i, cells));
            row_reader.next_row()?;
        }

        Ok(TableBuilder::new()
            .set_name(Some(Label::String(name)))
            .set_columns(columns)
            .set_rows(rows)
            .build())
    }

    fn as_slice(&self, range: RangeFrom<usize>) -> &[u8] {
        &self.data.get_ref()[range]
    }

    /// Reads a string from an absolute offset from the start of the table.
    fn read_string(&self, offset: usize) -> Result<Utf<'t>> {
        // TODO: use results?
        let res = match self.data.get_ref() {
            // To get a Utf of lifetime 't, we need to extract the 't slice from Cow::Borrowed,
            // or keep using owned values
            Cow::Owned(owned) => {
                let c_str =
                    CStr::from_bytes_until_nul(&owned[offset..]).expect("no string terminator");
                Ok(Cow::Owned(
                    c_str.to_str().expect("invalid utf8").to_string(),
                ))
            }
            Cow::Borrowed(borrowed) => {
                let c_str =
                    CStr::from_bytes_until_nul(&borrowed[offset..]).expect("no string terminator");
                Ok(Cow::Borrowed(c_str.to_str().expect("invalid utf8")))
            }
        };
        res
    }

    /// Reads a string relative to the names offset.
    fn read_name(&self, offset: usize) -> Result<Utf<'t>> {
        self.read_string(offset + self.header.offset_names)
    }
}

impl<'a, 't, E: ByteOrder> ColumnReader<'a, 't, E> {
    fn new(table: &'a TableReader<'t, E>, seek: u64) -> Result<Self> {
        let mut data = Cursor::new(table.data.get_ref());
        data.seek(SeekFrom::Start(seek))?;
        let info_ptr = data.read_u16::<E>()? as usize;
        Ok(Self {
            table,
            data,
            info_ptr,
            _endianness: PhantomData,
        })
    }

    fn read_column(mut self) -> Result<ColumnData<'a>> {
        self.data.read_u16::<E>()?; // TODO
        let name_offset = self.data.read_u16::<E>()?;
        let name = self.table.read_string(name_offset as usize)?;
        let cell = self.read_cell()?;
        Ok(ColumnData {
            name,
            cell,
            info_offset: self.info_ptr - 1,
        })
    }

    fn read_cell(&mut self) -> Result<ColumnCell> {
        // Flag, Value, Array
        let cell_type = self.data.get_ref()[self.info_ptr];
        self.info_ptr += 1;

        Ok(match cell_type {
            1 => ColumnCell::Value(self.read_value()?),
            2 => {
                let (val, sz) = self.read_array()?;
                ColumnCell::Array(val, sz)
            }
            3 => ColumnCell::Flag(self.read_flag()?),
            i => panic!("Unknown cell type {i}"), // TODO use error, also in XC3
        })
    }

    fn read_flag(&self) -> Result<FlagData> {
        let mut info_table = &self.data.get_ref()[self.info_ptr..];
        let flag_index = info_table.read_u8()?;
        let flag_mask = info_table.read_u32::<E>()?;
        let parent_offset = info_table.read_u16::<E>()? as usize;
        let parent_info_offset = (&self.data.get_ref()[parent_offset..]).read_u16::<E>()? as usize;
        Ok(FlagData {
            index: flag_index as usize,
            mask: flag_mask,
            parent_info_offset,
        })
    }

    fn read_value(&self) -> Result<ValueData> {
        let mut info_table = &self.data.get_ref()[self.info_ptr..];
        let value_type =
            ValueType::try_from(info_table.read_u8()?).expect("unsupported value type"); // TODO use error, also in XC3
        let value_offset = info_table.read_u16::<E>()?;
        Ok(ValueData {
            value_type,
            offset: value_offset as usize,
        })
    }

    fn read_array(&self) -> Result<(ValueData, usize)> {
        let mut info_table = &self.data.get_ref()[self.info_ptr..];
        let value_type =
            ValueType::try_from(info_table.read_u8()?).expect("unsupported value type");
        let value_offset = info_table.read_u16::<E>()?;
        let array_size = info_table.read_u16::<E>()?;
        Ok((
            ValueData {
                value_type,
                offset: value_offset as usize,
            },
            array_size as usize,
        ))
    }
}

impl<'a, 't, E: ByteOrder> RowReader<'a, 't, E> {
    fn new(table: &'a mut TableReader<'t, E>, columns: &'a [ColumnDef]) -> Self {
        Self {
            table,
            cells: vec![None; columns.len()],
            columns,
            row_idx: 0,
        }
    }

    fn next_row(&mut self) -> Result<()> {
        self.row_idx += 1;
        self.table.data.seek(SeekFrom::Start(
            (self.table.header.offset_rows + self.row_idx * self.table.header.row_len)
                .try_into()
                .unwrap(),
        ))?;
        self.cells.fill(None);
        Ok(())
    }

    fn read_row(&mut self) -> Result<Vec<Cell<'t>>> {
        for (i, col) in self.columns.iter().enumerate() {
            if col.count > 1 {
                // Array
                let values = self.read_array(col.value_type, col.count)?;
                self.cells[i] = Some(Cell::List(values));
                continue;
            }

            let value = self.read_value(col.value_type)?;

            if !col.flags.is_empty() {
                // Flags
                let value = value.into_integer();
                let flags = col.flags.iter().map(|f| value & f.mask).collect::<Vec<_>>();
                self.cells[i] = Some(Cell::Flags(flags));
                continue;
            }

            self.cells[i] = Some(Cell::Single(value));
        }

        Ok(self.cells.iter().flatten().cloned().collect())
    }

    fn read_value(&mut self, value_type: ValueType) -> Result<Value<'t>> {
        let buf = &mut self.table.data;
        Ok(match value_type {
            ValueType::Unknown => Value::Unknown,
            ValueType::UnsignedByte => Value::UnsignedByte(buf.read_u8()?),
            ValueType::UnsignedShort => Value::UnsignedShort(buf.read_u16::<E>()?),
            ValueType::UnsignedInt => Value::UnsignedInt(buf.read_u32::<E>()?),
            ValueType::SignedByte => Value::SignedByte(buf.read_i8()?),
            ValueType::SignedShort => Value::SignedShort(buf.read_i16::<E>()?),
            ValueType::SignedInt => Value::SignedInt(buf.read_i32::<E>()?),
            ValueType::String => {
                let offset = buf.read_u32::<E>()? as usize;
                // explicit return to get rid of the `buf` mutable borrow early
                return Ok(Value::String(self.table.read_string(offset)?));
            }
            ValueType::Float => Value::Float(match self.table.version {
                BdatVersion::LegacyX => BdatReal::Fixed(buf.read_u32::<E>()?.into()),
                _ => BdatReal::Floating(buf.read_f32::<E>()?.into()),
            }),
            _ => panic!("not supported in legacy bdat"), // TODO: results
        })
    }

    fn read_array(&mut self, value_type: ValueType, len: usize) -> Result<Vec<Value<'t>>> {
        (0..len).map(|_| self.read_value(value_type)).collect()
    }
}

impl ColumnCell {
    fn value(&self) -> &ValueData {
        match self {
            Self::Value(v) | Self::Array(v, _) => v,
            _ => panic!("value not supported"),
        }
    }

    fn is_flag(&self) -> bool {
        matches!(self, Self::Flag(_))
    }
}

impl<'t> Flags<'t> {
    fn new(mut src: Vec<ColumnData<'t>>) -> Self {
        src.sort_by_key(Self::extract);
        Self(src)
    }

    fn get_from_parent(&self, parent_info_offset: usize) -> impl Iterator<Item = &ColumnData> {
        let first_idx = self
            .0
            .binary_search_by_key(&parent_info_offset, Self::extract)
            .unwrap_or(self.0.len());
        self.0
            .iter()
            .skip(first_idx)
            .take_while(move |c| Self::extract(c) == parent_info_offset)
    }

    fn extract(column: &ColumnData<'_>) -> usize {
        match &column.cell {
            ColumnCell::Flag(f) => f.parent_info_offset,
            _ => panic!("not a flag"),
        }
    }
}

impl<'b, R: Read + Seek, E: ByteOrder> BdatFile<'b> for LegacyReader<R, E> {
    fn get_tables(&mut self) -> Result<Vec<Table<'b>>> {
        let mut tables = Vec::with_capacity(self.header.table_count);
        for offset in &self.header.table_offsets {
            self.reader.seek(SeekFrom::Start(*offset as u64))?;
            tables.push(TableReader::<E>::from_reader(&mut self.reader, self.version)?.read()?);
        }
        Ok(tables)
    }

    fn table_count(&self) -> usize {
        self.header.table_count
    }
}

impl<'b, E: ByteOrder> BdatFile<'b> for LegacySlice<'b, E> {
    fn get_tables(&mut self) -> Result<Vec<Table<'b>>> {
        let mut tables = Vec::with_capacity(self.header.table_count);
        for offset in &self.header.table_offsets {
            tables.push(TableReader::<E>::from_slice(&self.data[*offset..], self.version)?.read()?);
        }
        Ok(tables)
    }

    fn table_count(&self) -> usize {
        self.header.table_count
    }
}
