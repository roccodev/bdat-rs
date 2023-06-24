use std::io::{Cursor, Read, Seek, SeekFrom};

use byteorder::ReadBytesExt;

use crate::error::Result;
use crate::io::read::{BdatFile, BdatReader, BdatSlice};
use crate::io::BDAT_MAGIC;
use crate::legacy::read::{LegacyBytes, LegacyReader};
use crate::modern::FileReader;
use crate::{BdatVersion, SwitchEndian, Table, WiiEndian};

pub enum VersionReader<R: Read + Seek> {
    Legacy(LegacyReader<R, SwitchEndian>),
    LegacyX(LegacyReader<R, WiiEndian>),
    Modern(FileReader<BdatReader<R, SwitchEndian>, SwitchEndian>),
}

pub enum VersionSlice<'b> {
    Legacy(LegacyBytes<'b, SwitchEndian>),
    LegacyX(LegacyBytes<'b, WiiEndian>),
    Modern(FileReader<BdatSlice<'b, SwitchEndian>, SwitchEndian>),
}

/// Reads a BDAT file from a slice. The slice needs to have the **full** file data, though any
/// unrelated bytes at the end will be ignored.
///
/// Version and endianness will be automatically detected. To force a different endianness and/or
/// version, use the specialized functions from [`bdat::legacy`] and [`bdat::modern`].  
/// Notably, only the legacy implementation needs a mutable reference to the data (as it may
/// need to unscramble text), while this function is forced to carry that restriction, even when
/// effectively dealing with modern tables.
///
/// This function will only read the file header. To parse tables, call [`BdatFile::get_tables`].
///
/// ```
/// use std::fs::File;
/// use bdat::{BdatFile, BdatResult, SwitchEndian};
///
/// fn read(data: &mut [u8]) -> BdatResult<()> {
///     let tables = bdat::from_bytes(data)?.get_tables()?;
///     Ok(())
/// }
/// ```
pub fn from_bytes(bytes: &mut [u8]) -> Result<VersionSlice<'_>> {
    match detect_version(Cursor::new(&bytes))? {
        BdatVersion::Legacy => Ok(VersionSlice::Legacy(LegacyBytes::new(
            bytes,
            BdatVersion::Legacy,
        )?)),
        BdatVersion::LegacyX => Ok(VersionSlice::LegacyX(LegacyBytes::new(
            bytes,
            BdatVersion::LegacyX,
        )?)),
        BdatVersion::Modern => Ok(VersionSlice::Modern(
            FileReader::<_, SwitchEndian>::read_file(BdatSlice::<SwitchEndian>::new(bytes))?,
        )),
    }
}

/// Reads a BDAT file from a [`std::io::Read`] implementation. That type must also implement
/// [`std::io::Seek`].
///
/// Version and endianness will be automatically detected. To force a different endianness and/or
/// version, use the specialized functions from [`bdat::legacy`] and [`bdat::modern`].
///
/// This function will only read the file header. To parse tables, call [`BdatFile::get_tables`].
///
/// The BDAT file format is not recommended for streams, so it is best to read from a file or a
/// byte buffer.
///
/// ```
/// use std::fs::File;
/// use bdat::{BdatFile, BdatResult, SwitchEndian};
///
/// fn read_file(name: &str) -> BdatResult<()> {
///     let file = File::open(name)?;
///     let tables = bdat::from_reader(file)?.get_tables()?;
///     Ok(())
/// }
/// ```
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
    }
}

/// Attempts to detect the BDAT version used in the given slice. The slice must include the
/// full file header.
pub fn detect_bytes_version(bytes: &[u8]) -> Result<BdatVersion> {
    detect_version(Cursor::new(bytes))
}

/// Attempts to detect the BDAT version used in a file.
pub fn detect_file_version<R: Read + Seek>(reader: R) -> Result<BdatVersion> {
    detect_version(reader)
}

fn detect_version<R: Read + Seek>(mut reader: R) -> Result<BdatVersion> {
    let mut magic = [0u8; 4];
    reader.read_exact(&mut magic)?;
    if magic == BDAT_MAGIC {
        // XC3 BDAT files start with "BDAT"
        reader.seek(SeekFrom::Start(0))?;
        return Ok(BdatVersion::Modern);
    }

    // In other games, the magic space is the table count instead. By looking at how long
    // the table offset list is (reading until we meet "BDAT", which marks the start of the first
    // table), we can figure out endianness by checking against the table count.

    let file_size = reader.read_u32::<SwitchEndian>()?;

    if magic == [0, 0, 0, 0] {
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
    let mut new_magic = [0u8; 4];
    loop {
        reader.read_exact(&mut new_magic)?;
        if new_magic == BDAT_MAGIC {
            break;
        }
        actual_table_count += 1;
    }

    reader.seek(SeekFrom::Start(0))?;
    Ok(if actual_table_count == u32::from_le_bytes(magic) {
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
