use std::io::{Read, Seek, SeekFrom};

use self::read::BdatReader;
use crate::{error::Result, types::RawTable};

pub mod read;
pub mod write;

use byteorder::ByteOrder;
pub use byteorder::{BigEndian, LittleEndian, NativeEndian, NetworkEndian};

pub struct BdatFile<R, E> {
    reader: BdatReader<R, E>,
    pub(crate) header: FileHeader,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum BdatVersion {
    /// Used in XC1/XCX/XC2/XCDE
    Legacy,
    /// Used in XC3
    Modern,
}

#[derive(Debug)]
pub(crate) struct FileHeader {
    pub table_count: usize,
    pub(crate) table_offsets: Vec<usize>,
}

impl<R, E> BdatFile<R, E>
where
    R: Read + Seek,
    E: ByteOrder,
{
    pub fn read(input: R) -> Result<Self> {
        let mut reader = BdatReader::read_file(input)?;
        let header = reader.read_header()?;
        Ok(Self { reader, header })
    }

    pub fn table_count(&self) -> usize {
        self.header.table_count
    }

    pub fn get_tables(&mut self) -> Result<Vec<RawTable>> {
        let mut tables = Vec::with_capacity(self.header.table_count);

        for i in 0..self.header.table_count {
            self.reader
                .stream_mut()
                .seek(SeekFrom::Start(self.header.table_offsets[i] as u64))?;
            let table = self.reader.read_table()?;

            tables.push(table);
        }

        Ok(tables)
    }
}
