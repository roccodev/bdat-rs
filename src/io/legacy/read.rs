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

use byteorder::{ByteOrder, ReadBytesExt};

use crate::error::{Result, Scope};
use crate::legacy::scramble::{unscramble, ScrambleType};
use crate::{BdatError, ColumnDef, FlagDef, Label, Table, TableBuilder, ValueType};

use super::{FileHeader, TableHeader};

const COLUMN_DEF_LEN: usize = 6;
type Utf<'t> = Cow<'t, str>; // TODO: export to use in XC3 bdats

struct TableReader<'t, E> {
    header: TableHeader,
    data: Cursor<Cow<'t, [u8]>>,
    _endianness: PhantomData<E>,
}

struct ColumnReader<'a, 't, E> {
    table: &'a TableReader<'t, E>,
    data: Cursor<&'a Cow<'t, [u8]>>,
    info_ptr: usize,
    _endianness: PhantomData<E>,
}

#[derive(Debug)]
struct ColumnData<'t> {
    name: Utf<'t>,
    info_offset: usize,
    cell: ColumnCell,
}

#[derive(Debug)]
struct FlagData {
    index: usize,
    mask: u32,
    parent_info_offset: usize,
}

#[derive(Debug)]
struct ValueData {
    value_type: ValueType,
    offset: usize,
}

#[derive(Debug)]
enum ColumnCell {
    Flag(FlagData), // this is used in the flag's column, not the parent's
    Value(ValueData),
    Array(ValueData, usize), // array size
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

    pub fn for_each_table_mut<F, E>(&self, data: &mut [u8], f: F) -> std::result::Result<(), E>
    where
        F: Fn(&mut [u8]) -> std::result::Result<(), E>,
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

        Ok(())
    }
}

impl<'t, E: ByteOrder> TableReader<'t, E> {
    fn from_reader<R: Read + Seek>(mut reader: R) -> Result<Self> {
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
            data: Cursor::new(Cow::Owned(table_data)),
            _endianness: PhantomData,
        })
    }

    fn read(&mut self) -> Result<Table> {
        let name = self.read_name(0)?.to_string(); // TODO
        self.data.seek(SeekFrom::Start(
            self.header.offset_columns.try_into().unwrap(),
        ))?;
        let mut seek = self.data.position();
        let columns = (0..self.header.column_count)
            .map(|_| {
                let col = ColumnReader::new(self, seek)?.read_column()?;
                seek += COLUMN_DEF_LEN as u64;
                Ok(col)
            })
            .collect::<Result<Vec<_>>>()?;

        // De-flag-ify
        let columns = columns
            .iter()
            .filter(|c| c.cell.not_flag())
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
                    flags: columns
                        .iter()
                        .filter(|c1| matches!(&c1.cell, ColumnCell::Flag(f) if f.parent_info_offset == c.info_offset))
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
            .collect();

        self.data.seek(SeekFrom::Start(seek))?;

        Ok(TableBuilder::new()
            .set_name(Some(Label::String(name)))
            .set_columns(columns)
            .set_rows(vec![])
            .build())
    }

    fn as_slice(&self, range: RangeFrom<usize>) -> &[u8] {
        &self.data.get_ref()[range]
    }

    /// Reads a string from an absolute offset from the start of the table.
    fn read_string(&self, offset: usize) -> Result<Utf<'_>> {
        let c_str =
            CStr::from_bytes_until_nul(self.as_slice(offset..)).expect("no string terminator");
        Ok(Cow::Borrowed(c_str.to_str().expect("invalid utf8"))) // TODO use results?
    }

    /// Reads a string relative to the names offset.
    fn read_name(&self, offset: usize) -> Result<Utf<'_>> {
        let c_str = CStr::from_bytes_until_nul(self.as_slice(self.header.offset_names + offset..))
            .expect("no string terminator");
        Ok(Cow::Borrowed(c_str.to_str().expect("invalid utf8"))) // TODO use results?
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

impl TableHeader {
    pub fn read<E: ByteOrder>(mut reader: impl Read) -> Result<Self> {
        if reader.read_u32::<E>()? != 0x54_41_44_42 {
            // BDAT
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
                2 => ScrambleType::Scrambled(scramble_key),
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

impl ColumnCell {
    fn value(&self) -> &ValueData {
        match self {
            Self::Value(v) | Self::Array(v, _) => v,
            _ => panic!("value not supported"),
        }
    }

    fn not_flag(&self) -> bool {
        if let Self::Flag(_) = self {
            false
        } else {
            true
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::legacy::read::TableReader;
    use crate::legacy::FileHeader;
    use crate::{BdatError, SwitchEndian};
    use std::io::Cursor;

    #[test]
    fn test_columns() {
        let mut data = std::fs::read("/tmp/test.bdat").unwrap();
        let header = FileHeader::read::<_, SwitchEndian>(Cursor::new(data.as_slice())).unwrap();
        println!("Header {:?}", header);
        header
            .for_each_table_mut(&mut data, |table| {
                let mut reader = TableReader::<SwitchEndian>::from_reader(Cursor::new(table))?;
                println!("{:?}", reader.header);
                let table = reader.read()?;
                println!("\n That was table {:?} \n", table);
                Ok::<_, BdatError>(())
            })
            .unwrap();
        todo!()
    }
}
