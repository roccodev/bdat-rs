use std::io::{Cursor, Read, Seek, SeekFrom};

use byteorder::{ByteOrder, NativeEndian, ReadBytesExt};

use crate::error::Result;
use crate::io::read::{BdatFile, BdatReader, BdatSlice};
use crate::modern::FileReader;
use crate::{BdatVersion, SwitchEndian, Table};

pub enum VersionReader<R: Read + Seek, E: ByteOrder> {
    Legacy,
    Modern(FileReader<BdatReader<R, E>, E>),
}

pub enum VersionSlice<'b, E: ByteOrder> {
    Legacy,
    Modern(FileReader<BdatSlice<'b, E>, E>),
}

pub fn from_bytes(bytes: &[u8]) -> Result<VersionSlice<'_, impl ByteOrder>> {
    match detect_version(Cursor::new(bytes))? {
        BdatVersion::Legacy => Ok(VersionSlice::Legacy),
        BdatVersion::Modern => Ok(VersionSlice::Modern(
            FileReader::<_, SwitchEndian>::read_file(BdatSlice::<SwitchEndian>::new(bytes))?,
        )),
        _ => panic!(),
    }
}

pub fn from_reader<R: Read + Seek>(mut reader: R) -> Result<VersionReader<R, impl ByteOrder>> {
    match detect_version(&mut reader)? {
        BdatVersion::Legacy => Ok(VersionReader::Legacy),
        BdatVersion::Modern => Ok(VersionReader::Modern(
            FileReader::<_, SwitchEndian>::read_file(BdatReader::<_, SwitchEndian>::new(reader))?,
        )),
        _ => panic!(),
    }
}

fn detect_version<R: Read + Seek>(mut reader: R) -> Result<BdatVersion> {
    let magic = reader.read_u32::<NativeEndian>()?;
    if magic == 0x54_41_44_42 {
        // XC3 BDAT files start with "BDAT"
        reader.seek(SeekFrom::Start(0))?;
        return Ok(BdatVersion::Modern);
    }

    // In other games, the magic space is the table count instead. By looking at how long
    // the table offset list is (reading until we meet "BDAT", which marks the start of the first
    // table), we can figure out endianness by checking against the table count.

    let file_size = reader.read_u32::<NativeEndian>()?;

    if magic == 0 {
        // No tables, meaning we will have a very small file size. If the size is too large
        // it means we have the wrong endianness
        reader.seek(SeekFrom::Start(0))?;
        return Ok(if file_size > 1000 {
            BdatVersion::LegacyX
        } else {
            BdatVersion::Legacy
        });
    }

    let mut actual_table_count = 0;
    loop {
        let n = reader.read_u32::<NativeEndian>()?;
        if n == 0x54_41_44_42 {
            break;
        }
        actual_table_count += 1;
    }

    reader.seek(SeekFrom::Start(0))?;
    Ok(if actual_table_count == magic {
        BdatVersion::Legacy
    } else {
        BdatVersion::LegacyX
    })
}

impl<'b, R: Read + Seek, E: ByteOrder> BdatFile<'b> for VersionReader<R, E> {
    fn get_tables(&mut self) -> Result<Vec<Table<'b>>> {
        match self {
            Self::Legacy => Ok(vec![]),
            Self::Modern(r) => r.get_tables(),
        }
    }

    fn table_count(&self) -> usize {
        match self {
            Self::Legacy => 0,
            Self::Modern(r) => r.table_count(),
        }
    }
}

impl<'b, E: ByteOrder> BdatFile<'b> for VersionSlice<'b, E> {
    fn get_tables(&mut self) -> Result<Vec<Table<'b>>> {
        match self {
            Self::Legacy => Ok(vec![]),
            Self::Modern(r) => r.get_tables(),
        }
    }

    fn table_count(&self) -> usize {
        match self {
            Self::Legacy => 0,
            Self::Modern(r) => r.table_count(),
        }
    }
}
