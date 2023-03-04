use self::{read::BdatReader, write::BdatWriter};
use crate::{error::Result, types::RawTable};
use byteorder::ByteOrder;
use std::io::{Read, Seek, Write};

mod read;
mod write;

pub use byteorder::{BigEndian, LittleEndian, NativeEndian, NetworkEndian};
use crate::io::read::{BdatSlice, FileReader};

/// A little-endian [`BdatFile`] for the Nintendo Switch and Wii games
pub type SwitchBdatFile<R> = BdatFile<R, LittleEndian>;
/// A big-endian [`BdatFile`] for Xenoblade X (Wii U)
pub type WiiUBdatFile<R> = BdatFile<R, BigEndian>;

pub type SwitchEndian = LittleEndian;
pub type WiiEndian = BigEndian;

/// An interface over a stream ([`Read`] or [`Write`]) to read and write BDAT files.
///
/// Instead of using this type directly, consider using the endian-specific aliases
/// [`SwitchBdatFile`] and [`WiiUBdatFile`].
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
    Writer(BdatWriter<S, E>),
}

impl<R, E> BdatFile<R, E>
where
    R: Read + Seek,
    E: ByteOrder,
{
    /// Gets the number of tables in the file.
    ///
    /// This requires the file to be opened with [`BdatFile::new_read`], but this
    /// may change in the future.
    pub fn table_count(&self) -> usize {
        self.header
            .as_ref()
            .map(|h| h.table_count)
            .unwrap_or_default()
    }

    /// Reads all tables from the file.
    ///
    /// This requires the file to be opened with [`BdatFile::new_read`].
    pub fn get_tables(&mut self) -> Result<Vec<RawTable>> {
        /*
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

        Ok(tables)*/
        todo!()
    }
}

impl<W, E> BdatFile<W, E>
where
    W: Write + Seek,
    E: ByteOrder,
{
    /// Creates a file interface for writing.
    /// (This function does not write anything.)
    ///
    /// If read access is desired, use [`BdatFile::new_read`] instead.
    pub fn new_write(writer: W, version: BdatVersion) -> Self {
        Self {
            stream: BdatIo::Writer(BdatWriter::new(writer, version)),
            header: None,
        }
    }

    /// Writes all the tables, in order, to the stream. When the write is complete, the stream is flushed.
    ///
    /// This is only available if the file interface was created with [`BdatFile::new_write`].
    pub fn write_all_tables<'t>(&mut self, tables: impl IntoIterator<Item = RawTable<'t>>) -> Result<()> {
        match &mut self.stream {
            BdatIo::Writer(w) => w.write_file(tables),
            _ => panic!("unsupported write"),
        }
    }
}

pub fn from_reader<R: Read + Seek, E: ByteOrder>(reader: R) -> Result<FileReader<BdatReader<R, E>, E>> {
    FileReader::read_file(BdatReader::new(reader))
}

pub fn from_bytes<E: ByteOrder>(bytes: &[u8]) -> Result<FileReader<BdatSlice<'_, E>, E>> {
    FileReader::read_file(BdatSlice::new(bytes))
}

impl BdatVersion {
    /// Gets whether the version forces labels to be hashed.
    pub fn are_labels_hashed(&self) -> bool {
        *self == BdatVersion::Modern
    }
}
