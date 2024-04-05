use std::io::{Cursor, Read, Seek, SeekFrom};

use byteorder::{ByteOrder, ReadBytesExt};

use crate::compat::CompatTable;
use crate::error::Result;
use crate::io::read::{BdatFile, BdatReader, BdatSlice};
use crate::io::BDAT_MAGIC;
use crate::legacy::read::{LegacyBytes, LegacyReader};
use crate::modern::FileReader;
use crate::{BdatVersion, LegacyVersion, SwitchEndian, WiiEndian};

/// Compatibility file reader returned by [`bdat::from_reader`](`crate::from_reader`)
pub enum VersionReader<R: Read + Seek> {
    LegacyWii(LegacyReader<R, WiiEndian>),
    LegacySwitch(LegacyReader<R, SwitchEndian>),
    Modern(FileReader<BdatReader<R, SwitchEndian>, SwitchEndian>),
}

/// Compatibility slice reader returned by [`bdat::from_bytes`](`crate::from_bytes`)
pub enum VersionSlice<'b> {
    LegacyWii(LegacyBytes<'b, WiiEndian>),
    LegacySwitch(LegacyBytes<'b, SwitchEndian>),
    Modern(FileReader<BdatSlice<'b, SwitchEndian>, SwitchEndian>),
}

/// Errors that may occur while detecting the version of a BDAT file.
#[derive(thiserror::Error, Debug)]
pub enum DetectError {
    #[error("Not a BDAT file")]
    NotBdat,
    #[error("Can't determine legacy platform: no tables found")]
    LegacyNoTables,
}

/// Reads a BDAT file from a slice. The slice needs to have the **full** file data, though any
/// unrelated bytes at the end will be ignored.
///
/// This function will only read the file header. To parse tables, call [`BdatFile::get_tables`].
///
/// ## Version properties
///
/// Version and endianness will be automatically detected. To force a different endianness and/or
/// version, use the specialized functions from [`bdat::legacy`] and [`bdat::modern`].  
/// Notably, only the legacy implementation needs a mutable reference to the data (as it may
/// need to unscramble text), yet this function is forced to carry that restriction, even when
/// effectively dealing with modern tables.
///
/// Tables read using this function are compatible with most operations.  
/// If you know in advance that you are dealing with modern (XC3) or legacy (other games) tables,
/// you should use the specialized functions instead. That way, you can benefit from ergonomic
/// functions on the [`ModernTable`] and [`LegacyTable`] types.
///
/// ## Examples
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
///
/// [`bdat::legacy`]: crate::legacy
/// [`bdat::modern`]: crate::modern
/// [`BdatFile::get_tables`]: crate::BdatFile::get_tables
/// [`ModernTable`]: crate::modern::ModernTable
/// [`LegacyTable`]: crate::legacy::LegacyTable
pub fn from_bytes(bytes: &mut [u8]) -> Result<VersionSlice<'_>> {
    match detect_version(Cursor::new(&bytes))? {
        BdatVersion::Legacy(v @ LegacyVersion::Switch | v @ LegacyVersion::New3ds) => {
            Ok(VersionSlice::LegacySwitch(LegacyBytes::new(bytes, v)?))
        }
        BdatVersion::Legacy(v @ LegacyVersion::Wii | v @ LegacyVersion::X) => {
            Ok(VersionSlice::LegacyWii(LegacyBytes::new(bytes, v)?))
        }
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
///
/// [`bdat::legacy`]: crate::legacy
/// [`bdat::modern`]: crate::modern
/// [`BdatFile::get_tables`]: crate::BdatFile::get_tables
pub fn from_reader<R: Read + Seek>(mut reader: R) -> Result<VersionReader<R>> {
    let pos = reader.stream_position()?;
    let version = detect_version(&mut reader)?;
    reader.seek(SeekFrom::Start(pos))?;
    match version {
        BdatVersion::Legacy(v @ LegacyVersion::Switch | v @ LegacyVersion::New3ds) => {
            Ok(VersionReader::LegacySwitch(LegacyReader::new(reader, v)?))
        }
        BdatVersion::Legacy(v @ LegacyVersion::Wii | v @ LegacyVersion::X) => {
            Ok(VersionReader::LegacyWii(LegacyReader::new(reader, v)?))
        }
        BdatVersion::Modern => Ok(VersionReader::Modern(
            FileReader::<_, SwitchEndian>::read_file(BdatReader::<_, SwitchEndian>::new(reader))?,
        )),
    }
}

/// Attempts to detect the BDAT version used in the given slice. The slice must include the
/// full file header.
///
/// An error ([`BdatError::VersionDetect`]) might be returned if the version couldn't be detected
/// because of ambiguous details.
///
/// [`BdatError::VersionDetect`]: crate::BdatError::VersionDetect
pub fn detect_bytes_version(bytes: &[u8]) -> Result<BdatVersion> {
    detect_version(Cursor::new(bytes))
}

/// Attempts to detect the BDAT version used in a file.
///
/// An error ([`BdatError::VersionDetect`]) might be returned if the version couldn't be detected
/// because of ambiguous details.
///
/// **Note**: the state of the reader will be modified after the call.
///
/// [`BdatError::VersionDetect`]: crate::BdatError::VersionDetect
pub fn detect_file_version<R: Read + Seek>(reader: R) -> Result<BdatVersion> {
    detect_version(reader)
}

fn detect_version<R: Read + Seek>(mut reader: R) -> Result<BdatVersion> {
    let mut magic = [0u8; 4];
    reader.read_exact(&mut magic)?;
    if magic == BDAT_MAGIC {
        // XC3 BDAT files start with "BDAT"
        return Ok(BdatVersion::Modern);
    }

    // In other games, the magic space is the table count instead. By looking at how long
    // the table offset list is (reading until we meet "BDAT", which marks the start of the first
    // table), we can figure out endianness by checking against the table count.
    const MAGIC_INT: u32 = u32::from_le_bytes(BDAT_MAGIC);

    if magic == [0, 0, 0, 0] {
        // No tables, we can't distinguish. Prior to 0.5, this used to be allowed for little endian
        // tables, because proper XC3D tables weren't supported yet.
        return Err(DetectError::LegacyNoTables.into());
    }

    let mut actual_table_count = 0;
    let mut new_magic = [0u8; 4];
    let mut first_offset = 0;
    reader.seek(SeekFrom::Current(4))?;
    loop {
        reader.read_exact(&mut new_magic)?;
        // Read until padding, "BDAT", or "TADB" (3DS)
        if new_magic == [0, 0, 0, 0]
            || SwitchEndian::read_u32(&new_magic) == MAGIC_INT
            || WiiEndian::read_u32(&new_magic) == MAGIC_INT
        {
            break;
        }
        if first_offset == 0 {
            first_offset = WiiEndian::read_u32(&new_magic);
        }
        actual_table_count += 1;
    }

    reader.seek(SeekFrom::Start(0))?;
    let expected_table_count = SwitchEndian::read_u32(&magic);
    if actual_table_count == expected_table_count {
        // `first_offset` was first read as big endian, but if the table count matches we
        // need little endian (either 3DS or Switch)
        reader.seek(SeekFrom::Start(
            SwitchEndian::read_u32(&first_offset.to_be_bytes()) as u64,
        ))?;
        reader.read_exact(&mut new_magic)?;
        if WiiEndian::read_u32(&new_magic) == MAGIC_INT {
            // Table magic in big endian, this is a 3DS file.
            return Ok(LegacyVersion::New3ds.into());
        } else if SwitchEndian::read_u32(&new_magic) == MAGIC_INT {
            return Ok(LegacyVersion::Switch.into());
        }
        return Err(DetectError::NotBdat.into());
    }

    // If we've reached this point, we either have a XC1 (Wii) file or a XCX file, which are both
    // big-endian formats.
    // In XC1, headers are only 32 bytes long
    //
    // To disambiguate, we check the 16-bit value at table+32 and the 32-bit value at table+36.
    // In XCX, table+32 is the address of the first column node, while in XC1 this can either be:
    // - The first column info (starting with either 0x01, 0x02, or 0x03 for the cell type)
    // - A string from the name table, if there are no columns
    // No other data can be at that location, because if there are no columns, there are also
    // no rows and no strings.
    //
    // In XCX, table+36 is always [0; 4]. In XC1, this can also be [0; 4] if e.g. table+32 contains
    // a string (the table name) that is 5 bytes long (4+nul), as padding would add
    // 3 extra zeroes at the end.
    //
    // Table+32 is guaranteed to exist, because all tables need a name and the shortest name you
    // can have is '\0'. If any location between +32 and +36 doesn't exist, then it's 100% a XC1
    // table.
    //
    // If table+36 is [0; 4] and table+32 (16-bit) is a valid offset (i.e. <= string table max
    // offset), then the table is from XCX.
    // In any other case, it's the XC1 format.

    reader.seek(SeekFrom::Start(first_offset as u64))?;
    // Magic is always BDAT for non-3DS games
    if reader.read_u32::<SwitchEndian>()? != MAGIC_INT {
        return Err(DetectError::NotBdat.into());
    }
    reader.seek(SeekFrom::Current(32 - 4 * 3))?;
    let string_table_offset = reader.read_u32::<WiiEndian>()?;
    let string_table_len = reader.read_u32::<WiiEndian>()?;
    let final_offset = string_table_offset + string_table_len;

    if first_offset + 36 > final_offset {
        return Ok(LegacyVersion::Wii.into());
    }

    let t_32 = reader.read_u32::<WiiEndian>()? >> 16;
    let t_36 = reader.read_u32::<WiiEndian>()?;
    Ok(match (t_32, t_36) {
        (x, 0) if x <= final_offset => LegacyVersion::X.into(),
        (_, _) => LegacyVersion::Wii.into(),
    })
}

impl<'b, R: Read + Seek> BdatFile<'b> for VersionReader<R> {
    type TableOut = CompatTable<'b>;

    fn get_tables(&mut self) -> crate::error::Result<Vec<CompatTable<'b>>> {
        match self {
            Self::LegacySwitch(r) => r
                .get_tables()
                .map(|v| v.into_iter().map(Into::into).collect()),
            Self::LegacyWii(r) => r
                .get_tables()
                .map(|v| v.into_iter().map(Into::into).collect()),
            Self::Modern(r) => r
                .get_tables()
                .map(|v| v.into_iter().map(Into::into).collect()),
        }
    }

    fn table_count(&self) -> usize {
        match self {
            Self::LegacySwitch(r) => r.table_count(),
            Self::LegacyWii(r) => r.table_count(),
            Self::Modern(r) => r.table_count(),
        }
    }
}

impl<'b> BdatFile<'b> for VersionSlice<'b> {
    type TableOut = CompatTable<'b>;

    fn get_tables(&mut self) -> crate::error::Result<Vec<CompatTable<'b>>> {
        match self {
            Self::LegacySwitch(r) => r
                .get_tables()
                .map(|v| v.into_iter().map(Into::into).collect()),
            Self::LegacyWii(r) => r
                .get_tables()
                .map(|v| v.into_iter().map(Into::into).collect()),
            Self::Modern(r) => r
                .get_tables()
                .map(|v| v.into_iter().map(Into::into).collect()),
        }
    }

    fn table_count(&self) -> usize {
        match self {
            Self::LegacySwitch(r) => r.table_count(),
            Self::LegacyWii(r) => r.table_count(),
            Self::Modern(r) => r.table_count(),
        }
    }
}
