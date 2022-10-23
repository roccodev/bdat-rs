use self::{read::BdatReader, write::BdatWriter};
use crate::{error::Result, types::RawTable};
use byteorder::ByteOrder;
use std::io::{Read, Seek, SeekFrom, Write};

mod read;
mod write;

pub use byteorder::{BigEndian, LittleEndian, NativeEndian, NetworkEndian};

/// A little-endian BDAT file for the Nintendo Switch and Wii games
pub type SwitchBdatFile<R> = BdatFile<R, LittleEndian>;
/// A big-endian BDAT file for Xenoblade X
pub type WiiUBdatFile<R> = BdatFile<R, BigEndian>;

pub struct BdatFile<S, E> {
    stream: BdatIo<S, E>,
    header: Option<FileHeader>,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
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

enum BdatIo<S, E> {
    Reader(BdatReader<S, E>),
    Writer(BdatWriter<S, E>),
}

impl<R, E> BdatFile<R, E>
where
    R: Read + Seek,
    E: ByteOrder,
{
    pub fn read(input: R) -> Result<Self> {
        let mut reader = BdatReader::read_file(input)?;
        let header = reader.read_header()?;
        Ok(Self {
            stream: BdatIo::Reader(reader),
            header: Some(header),
        })
    }

    pub fn table_count(&self) -> usize {
        self.header
            .as_ref()
            .map(|h| h.table_count)
            .unwrap_or_default()
    }

    pub fn get_tables(&mut self) -> Result<Vec<RawTable>> {
        let (reader, header) = match (&mut self.stream, &self.header) {
            (BdatIo::Reader(r), Some(header)) => (r, header),
            _ => panic!("unsupported read"),
        };

        let mut tables = Vec::with_capacity(header.table_count);

        for i in 0..header.table_count {
            reader
                .stream_mut()
                .seek(SeekFrom::Start(header.table_offsets[i] as u64))?;
            let table = reader.read_table()?;

            tables.push(table);
        }

        Ok(tables)
    }
}

impl<W, E> BdatFile<W, E>
where
    W: Write + Seek,
    E: ByteOrder,
{
    pub fn new(writer: W, version: BdatVersion) -> Self {
        Self {
            stream: BdatIo::Writer(BdatWriter::new(writer, version)),
            header: None,
        }
    }

    pub fn write_all_tables(&mut self, tables: impl IntoIterator<Item = RawTable>) -> Result<()> {
        match &mut self.stream {
            BdatIo::Writer(w) => w.write_file(tables),
            _ => panic!("unsupported write"),
        }
    }
}

impl BdatVersion {
    pub fn are_labels_hashed(&self) -> bool {
        *self == BdatVersion::Modern
    }
}
