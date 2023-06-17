use std::io::{Cursor, Read, Seek, SeekFrom};

use byteorder::{NativeEndian, ReadBytesExt};

use crate::error::Result;
use crate::io::read::{BdatFile, BdatReader, BdatSlice};
use crate::legacy::read::{LegacyReader, LegacySlice};
use crate::modern::FileReader;
use crate::{BdatVersion, SwitchEndian, Table, WiiEndian};

pub enum VersionReader<R: Read + Seek> {
    Legacy(LegacyReader<R, SwitchEndian>),
    LegacyX(LegacyReader<R, WiiEndian>),
    Modern(FileReader<BdatReader<R, SwitchEndian>, SwitchEndian>),
}

pub enum VersionSlice<'b> {
    Legacy(LegacySlice<'b, SwitchEndian>),
    LegacyX(LegacySlice<'b, WiiEndian>),
    Modern(FileReader<BdatSlice<'b, SwitchEndian>, SwitchEndian>),
}

pub fn from_bytes(bytes: &mut [u8]) -> Result<VersionSlice<'_>> {
    match detect_version(Cursor::new(&bytes))? {
        BdatVersion::Legacy => Ok(VersionSlice::Legacy(LegacySlice::new(
            bytes,
            BdatVersion::Legacy,
        )?)),
        BdatVersion::LegacyX => Ok(VersionSlice::LegacyX(LegacySlice::new(
            bytes,
            BdatVersion::LegacyX,
        )?)),
        BdatVersion::Modern => Ok(VersionSlice::Modern(
            FileReader::<_, SwitchEndian>::read_file(BdatSlice::<SwitchEndian>::new(bytes))?,
        )),
        _ => panic!(),
    }
}

pub fn from_reader<R: Read + Seek>(mut reader: R) -> Result<VersionReader<R>> {
    match detect_version(&mut reader)? {
        BdatVersion::Legacy => Ok(VersionReader::Legacy(LegacyReader::new(
            reader,
            BdatVersion::Legacy,
        )?)),
        BdatVersion::LegacyX => Ok(VersionReader::LegacyX(LegacyReader::new(
            reader,
            BdatVersion::LegacyX,
        )?)),
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

impl<'b, R: Read + Seek> BdatFile<'b> for VersionReader<R> {
    fn get_tables(&mut self) -> Result<Vec<Table<'b>>> {
        match self {
            Self::Legacy(r) => r.get_tables(),
            Self::LegacyX(r) => r.get_tables(),
            Self::Modern(r) => r.get_tables(),
        }
    }

    fn table_count(&self) -> usize {
        match self {
            Self::Legacy(r) => r.table_count(),
            Self::LegacyX(r) => r.table_count(),
            Self::Modern(r) => r.table_count(),
        }
    }
}

impl<'b> BdatFile<'b> for VersionSlice<'b> {
    fn get_tables(&mut self) -> Result<Vec<Table<'b>>> {
        match self {
            Self::Legacy(r) => r.get_tables(),
            Self::LegacyX(r) => r.get_tables(),
            Self::Modern(r) => r.get_tables(),
        }
    }

    fn table_count(&self) -> usize {
        match self {
            Self::Legacy(r) => r.table_count(),
            Self::LegacyX(r) => r.table_count(),
            Self::Modern(r) => r.table_count(),
        }
    }
}
