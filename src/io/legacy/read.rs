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

use super::{FileHeader, TableHeader};
use crate::error::{Result, Scope};
use crate::legacy::scramble::{unscramble, ScrambleType};
use crate::BdatError;
use byteorder::{ByteOrder, ReadBytesExt};
use std::io::{Read, Seek, SeekFrom};

const COLUMN_DEF_LEN: usize = 6;

enum TableData<R> {
    Unscrambled(Vec<u8>),
    Verbatim(R),
}

struct TableReader<R> {
    header: TableHeader,
    data: TableData<R>,
}

impl FileHeader {
    pub fn read<R: Read + Seek, E: ByteOrder>(mut reader: R) -> Result<Self> {
        let table_count = reader.read_u32::<E>()? as usize;
        let file_size = reader.read_u32::<E>()? as usize;
        let mut offsets = Vec::with_capacity(table_count as usize);
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

impl<R: Read + Seek> TableReader<R> {
    fn from_reader<E: ByteOrder>(mut reader: R) -> Result<Self> {
        let original_pos = reader.stream_position()?;
        let header = TableHeader::read::<E>(&mut reader)?;
        reader.seek(SeekFrom::Start(original_pos))?;

        let scramble_key = match header.scramble_type {
            ScrambleType::Scrambled(key) => key,
            ScrambleType::Unknown => panic!("Unknown scramble type"),
            // We don't need to unscramble, return early without copying
            ScrambleType::None => {
                return Ok(Self {
                    header,
                    data: TableData::Verbatim(reader),
                })
            }
        };

        let table_len = header.get_table_len();
        let mut table_data: Vec<u8> = Vec::with_capacity(table_len);
        let bytes_read = reader
            .take(table_len.try_into().unwrap())
            .read_to_end(&mut table_data)?;
        if bytes_read != table_len {
            todo!("unexpected eof");
        }

        header.unscramble_data(&mut table_data);

        Ok(Self {
            header,
            data: TableData::Unscrambled(table_data),
        })
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
