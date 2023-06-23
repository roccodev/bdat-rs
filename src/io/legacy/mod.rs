pub mod float;
pub mod scramble;

mod hash;
pub(crate) mod read;
mod util;
mod write;

use byteorder::ByteOrder;
use scramble::ScrambleType;
use std::borrow::Borrow;
use std::io::{Cursor, Seek, Write};
use std::ops::Range;

use crate::error::Result;
use crate::legacy::write::FileWriter;
use crate::{BdatVersion, Table};

const HEADER_SIZE: usize = 64;
const COLUMN_DEFINITION_SIZE: usize = 6;

#[derive(Debug)]
pub struct FileHeader {
    pub table_count: usize,
    file_size: usize,
    table_offsets: Vec<usize>,
}

#[derive(Debug)]
pub struct TableHeader {
    pub scramble_type: ScrambleType,
    hashes: OffsetAndLen,
    strings: OffsetAndLen,
    offset_names: usize,
    offset_columns: usize,
    offset_rows: usize,
    column_count: usize,
    row_count: usize,
    row_len: usize,
    base_id: usize,
}

#[derive(Debug)]
struct OffsetAndLen {
    offset: usize,
    len: usize,
}

impl OffsetAndLen {
    fn max_offset(&self) -> usize {
        self.offset + self.len
    }

    fn range(&self) -> Range<usize> {
        self.offset..self.offset + self.len
    }
}

impl From<(usize, usize)> for OffsetAndLen {
    fn from((offset, len): (usize, usize)) -> Self {
        Self { offset, len }
    }
}

/// Writes legacy BDAT tables to a [`std::io::Write`] implementation
/// that also implements [`std::io::Seek`].
///
/// ```
/// use std::fs::File;
/// use bdat::{BdatResult, Table, SwitchEndian, BdatVersion};
///
/// fn write_file(name: &str, tables: &[Table]) -> BdatResult<()> {
///     let file = File::create(name)?;
///     // The legacy writer supports BdatVersion::Legacy and BdatVersion::LegacyX
///     bdat::legacy::to_writer::<_, SwitchEndian>(file, tables, BdatVersion::Legacy)?;
///     Ok(())
/// }
/// ```
pub fn to_writer<'t, W: Write + Seek, E: ByteOrder>(
    writer: W,
    tables: impl IntoIterator<Item = impl Borrow<Table<'t>>>,
    version: BdatVersion,
) -> Result<()> {
    let mut writer = FileWriter::<W, E>::new(writer, version);
    writer.write_file(tables)
}

/// Writes legacy BDAT tables to a `Vec<u8>`.
///
/// ```
/// use std::fs::File;
/// use bdat::{BdatResult, Table, SwitchEndian, BdatVersion};
///
/// fn write_vec(tables: &[Table]) -> BdatResult<()> {
///     // The legacy writer supports BdatVersion::Legacy and BdatVersion::LegacyX
///     let vec = bdat::legacy::to_vec::<SwitchEndian>(tables, BdatVersion::Legacy)?;
///     Ok(())
/// }
/// ```
pub fn to_vec<'t, E: ByteOrder>(
    tables: impl IntoIterator<Item = impl Borrow<Table<'t>>>,
    version: BdatVersion,
) -> Result<Vec<u8>> {
    let mut vec = Vec::new();
    to_writer::<_, E>(Cursor::new(&mut vec), tables, version)?;
    Ok(vec)
}
