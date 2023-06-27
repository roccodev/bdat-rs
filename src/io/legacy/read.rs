use std::borrow::Cow;
use std::collections::{HashSet, VecDeque};
use std::ffi::CStr;
use std::io::{Cursor, Read, Seek, SeekFrom};
use std::marker::PhantomData;

use byteorder::{ByteOrder, ReadBytesExt};

use crate::error::{Result, Scope};
use crate::io::BDAT_MAGIC;
use crate::legacy::float::BdatReal;
use crate::legacy::scramble::{unscramble, ScrambleType};
use crate::legacy::{ColumnNodeInfo, COLUMN_NODE_SIZE};
use crate::types::Utf;
use crate::{
    BdatError, BdatFile, BdatVersion, Cell, ColumnDef, FlagDef, Label, Row, Table, TableBuilder,
    Value, ValueType,
};

use super::{FileHeader, TableHeader};

pub struct LegacyBytes<'t, E> {
    data: Cow<'t, [u8]>,
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
    data: &'a Cow<'t, [u8]>,
    node_offset: u64,
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

#[derive(Debug)]
struct TableColumns<'t> {
    columns: Vec<ColumnData<'t>>,
    flags: Flags<'t>,
}

#[derive(Debug)]
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

impl<'t, E: ByteOrder> LegacyBytes<'t, E> {
    pub fn new(bytes: &'t mut [u8], version: BdatVersion) -> Result<Self> {
        let header = FileHeader::read::<_, E>(Cursor::new(&bytes))?;
        // TODO
        header.for_each_table_mut(bytes, |table| {
            let header = TableHeader::read::<E>(Cursor::new(&table), version)?;
            header.unscramble_data(table);
            table[4] = 0;
            Ok::<_, BdatError>(())
        })?;
        Ok(Self {
            header,
            version,
            data: Cow::Borrowed(bytes),
            _endianness: PhantomData,
        })
    }

    pub fn new_copy(bytes: &[u8], version: BdatVersion) -> Result<Self> {
        let header = FileHeader::read::<_, E>(Cursor::new(&bytes))?;
        Ok(Self {
            header,
            version,
            data: Cow::Owned(bytes.to_vec()),
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
    pub fn read<E: ByteOrder>(mut reader: impl Read, version: BdatVersion) -> Result<Self> {
        let mut magic = [0u8; 4];
        reader.read_exact(&mut magic)?;
        if magic != BDAT_MAGIC {
            // BDAT - doesn't change with endianness
            return Err(BdatError::MalformedBdat(Scope::Table));
        }
        let scramble_id = reader.read_u16::<E>()? as usize;
        let offset_names = reader.read_u16::<E>()? as usize;
        let row_len = reader.read_u16::<E>()? as usize;
        let offset_hashes = reader.read_u16::<E>()? as usize;
        let hash_slot_count = reader.read_u16::<E>()? as usize;
        let offset_rows = reader.read_u16::<E>()? as usize;
        let row_count = reader.read_u16::<E>()? as usize;
        let base_id = reader.read_u16::<E>()? as usize;
        assert_eq!(2, reader.read_u16::<E>()?, "unknown constant is not 2");
        let scramble_key = reader.read_u16::<E>()?;
        let offset_strings = reader.read_u32::<E>()? as usize;
        let strings_len = reader.read_u32::<E>()? as usize;
        let columns = if version != BdatVersion::LegacyWii {
            let offset_columns = reader.read_u16::<E>()? as usize;
            let column_count = reader.read_u16::<E>()? as usize;
            Some(ColumnNodeInfo {
                offset_columns,
                column_count,
            })
        } else {
            None
        };

        Ok(Self {
            scramble_type: match scramble_id {
                0 => ScrambleType::None,
                0x300 /* XCX */ | 2 => ScrambleType::Scrambled(scramble_key),
                s => return Err(BdatError::UnknownScrambleType(s as u16)),
            },
            hashes: (offset_hashes, hash_slot_count * 2).into(),
            strings: (offset_strings, strings_len).into(),
            offset_names,
            offset_rows,
            row_count,
            row_len,
            base_id,
            columns,
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
        // All legacy games expect the table length to be determined by the last byte
        // of the string table. (see Bdat::calcCheckSum)
        self.strings.max_offset()
    }
}

impl<'t, E: ByteOrder> TableReader<'t, E> {
    fn from_reader<R: Read + Seek>(mut reader: R, version: BdatVersion) -> Result<Self> {
        let original_pos = reader.stream_position()?;
        let header = TableHeader::read::<E>(&mut reader, version)?;
        reader.seek(SeekFrom::Start(original_pos))?;

        let table_len = header.get_table_len();
        let mut table_data: Vec<u8> = Vec::with_capacity(table_len);
        let bytes_read = reader
            .take(table_len.try_into()?)
            .read_to_end(&mut table_data)?;
        if bytes_read != table_len {
            return Err(eof(()));
        }

        match header.scramble_type {
            ScrambleType::Scrambled(_) => header.unscramble_data(&mut table_data),
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
        let header = TableHeader::read::<E>(&mut reader, version)?;
        reader.seek(SeekFrom::Start(original_pos))?;

        Ok(Self {
            header,
            version,
            data: Cursor::new(Cow::Borrowed(bytes)),
            _endianness: PhantomData,
        })
    }

    fn read(mut self) -> Result<Table<'t>> {
        let name = self.read_string(self.header.offset_names)?.to_string();
        let TableColumns {
            columns: columns_src,
            flags,
        } = match self.header.columns.as_ref() {
            Some(info) => self.discover_columns_from_nodes(info),
            None => self.discover_columns_from_hash(),
        }?;

        // De-flag-ify
        let columns = columns_src
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
                                label: f.name.to_string(),
                                flag_index: flag.index,
                                mask: flag.mask,
                            }
                        })
                        .collect(),
                }
            })
            .collect::<Vec<_>>();

        self.data
            .seek(SeekFrom::Start(self.header.offset_rows.try_into()?))?;

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

    fn discover_columns_from_nodes(&self, info: &ColumnNodeInfo) -> Result<TableColumns> {
        let mut seek = info.offset_columns.try_into()?;
        let (flags, columns) = (0..info.column_count)
            .map(|_| {
                let col = ColumnReader::new(self, seek).read_column_from_node()?;
                seek += COLUMN_NODE_SIZE as u64;
                Ok(col)
            })
            .partition::<Vec<_>, _>(|c| {
                c.as_ref()
                    .ok()
                    .and_then(|c| c.cell.is_flag().then_some(()))
                    .is_some()
            });
        Ok(TableColumns {
            flags: Flags::new(flags.into_iter().collect::<Result<_>>()?),
            columns: columns.into_iter().collect::<Result<_>>()?,
        })
    }

    fn discover_columns_from_hash(&self) -> Result<TableColumns> {
        // In XC1, column nodes are part of the name table, but we can enumerate columns
        // from the hash table, so we get easy access to both info data and name

        let mut to_visit = self.data.get_ref()[self.header.hashes.range()]
            .chunks_exact(2)
            .map(|b| E::read_u16(b) as usize)
            .filter(|&i| i != 0)
            .collect::<VecDeque<_>>();
        let mut visited = to_visit.iter().copied().collect::<HashSet<_>>(); // safeguard

        let (mut columns, mut flags) = (vec![], vec![]);

        while let Some(node_ptr) = to_visit.pop_front() {
            let (column, next) =
                ColumnReader::new(self, node_ptr.try_into()?).read_column_from_hash_node()?;
            if next != 0 && visited.insert(next) {
                to_visit.push_back(next);
            }
            if column.cell.is_flag() {
                flags.push(column);
            } else {
                columns.push(column);
            }
        }

        columns.sort_unstable_by_key(|c| c.cell.value().offset);
        flags.sort_unstable_by_key(|f| f.info_offset);

        Ok(TableColumns {
            flags: Flags::new(flags),
            columns,
        })
    }

    /// Reads a string from an absolute offset from the start of the table.
    fn read_string(&self, offset: usize) -> Result<Utf<'t>> {
        let res = match self.data.get_ref() {
            // To get a Utf of lifetime 't, we need to extract the 't slice from Cow::Borrowed,
            // or keep using owned values
            Cow::Owned(owned) => {
                let c_str = CStr::from_bytes_until_nul(&owned[offset..]).map_err(eof)?;
                Ok(Cow::Owned(c_str.to_str()?.to_string()))
            }
            Cow::Borrowed(borrowed) => {
                let c_str = CStr::from_bytes_until_nul(&borrowed[offset..]).map_err(eof)?;
                Ok(Cow::Borrowed(c_str.to_str()?))
            }
        };
        res
    }
}

impl<'a, 't: 'a, E: ByteOrder + 'a> ColumnReader<'a, 't, E> {
    fn new(table: &'a TableReader<'t, E>, node_offset: u64) -> Self {
        Self {
            table,
            data: table.data.get_ref(),
            node_offset,
            _endianness: PhantomData,
        }
    }

    fn read_column_from_node(self) -> Result<ColumnData<'a>> {
        let mut data = Cursor::new(self.data);
        data.set_position(self.node_offset);
        let info_ptr = data.read_u16::<E>()? as u64;
        data.read_u16::<E>()?; // hash table linked node
        let name_offset = data.read_u16::<E>()?;

        let cell = self.read_cell(info_ptr)?;
        let name = self.table.read_string(name_offset as usize)?;

        Ok(ColumnData {
            name,
            cell,
            info_offset: info_ptr as usize,
        })
    }

    /// Wii only.
    fn read_column_from_hash_node(self) -> Result<(ColumnData<'a>, usize)> {
        let mut data = Cursor::new(self.data);
        data.set_position(self.node_offset);
        let info_ptr = data.read_u16::<E>()? as u64;
        let next = data.read_u16::<E>()? as usize; // hash table linked node

        // Not a pointer, the string is embedded here.
        let name = self.table.read_string(data.position() as usize)?;
        let cell = self.read_cell(info_ptr)?;

        Ok((
            ColumnData {
                name,
                cell,
                info_offset: info_ptr as usize,
            },
            next,
        ))
    }

    fn read_cell(&self, info_ptr: u64) -> Result<ColumnCell> {
        let mut info_table = Cursor::new(self.data);
        info_table.set_position(info_ptr);

        // Flag, Value, Array
        let cell_type = info_table.read_u8()?;

        Ok(match cell_type {
            1 => ColumnCell::Value(Self::read_value(info_table)?),
            2 => {
                let (val, sz) = Self::read_array(info_table)?;
                ColumnCell::Array(val, sz)
            }
            3 => ColumnCell::Flag(Self::read_flag(info_table, self.data)?),
            i => return Err(BdatError::UnknownCellType(i)),
        })
    }

    fn read_flag(mut info_table: impl Read, full_table: &[u8]) -> Result<FlagData> {
        let flag_index = info_table.read_u8()?;
        let flag_mask = info_table.read_u32::<E>()?;
        let parent_offset = info_table.read_u16::<E>()? as usize;
        let parent_info_offset = (&full_table[parent_offset..]).read_u16::<E>()? as usize;
        Ok(FlagData {
            index: flag_index as usize,
            mask: flag_mask,
            parent_info_offset,
        })
    }

    fn read_value(mut info_table: impl Read) -> Result<ValueData> {
        let value_type = info_table.read_u8()?;
        let value_type =
            ValueType::try_from(value_type).map_err(|_| BdatError::UnknownValueType(value_type))?;
        let value_offset = info_table.read_u16::<E>()?;
        Ok(ValueData {
            value_type,
            offset: value_offset as usize,
        })
    }

    fn read_array(mut info_table: impl Read) -> Result<(ValueData, usize)> {
        let value_type = info_table.read_u8()?;
        let value_type =
            ValueType::try_from(value_type).map_err(|_| BdatError::UnknownValueType(value_type))?;
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
                let flags = col
                    .flags
                    .iter()
                    .map(|f| (value & f.mask) >> f.flag_index)
                    .collect::<Vec<_>>();
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
            ValueType::Float => Value::Float(BdatReal::from_bits(
                buf.read_u32::<E>()?,
                self.table.version,
            )),
            t => return Err(BdatError::UnsupportedType(t, self.table.version)),
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
        self.0
            .iter()
            .skip_while(move |c| Self::extract(c) != parent_info_offset)
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

impl<'b, E: ByteOrder> BdatFile<'b> for LegacyBytes<'b, E> {
    fn get_tables(&mut self) -> Result<Vec<Table<'b>>> {
        let mut tables = Vec::with_capacity(self.header.table_count);
        for offset in &self.header.table_offsets {
            tables.push(match &self.data {
                Cow::Owned(buf) => {
                    TableReader::<E>::from_reader(Cursor::new(&buf[*offset..]), self.version)?
                        .read()?
                }
                Cow::Borrowed(data) => {
                    TableReader::<E>::from_slice(&data[*offset..], self.version)?.read()?
                }
            });
        }
        Ok(tables)
    }

    fn table_count(&self) -> usize {
        self.header.table_count
    }
}

#[inline]
fn eof<T>(_: T) -> BdatError {
    std::io::Error::new(
        std::io::ErrorKind::UnexpectedEof,
        "failed to fill whole buffer",
    )
    .into()
}
