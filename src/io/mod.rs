use std::borrow::Borrow;
use std::io::{Cursor, Read, Seek, Write};

use byteorder::ByteOrder;

use crate::io::read::BdatSlice;
use crate::{error::Result, types::Table};

use self::{read::BdatReader, write::BdatWriter};

mod read;
mod write;

pub use read::FileReader;

/// Alias for [`byteorder::LittleEndian`], i.e. the byte order used in the Switch games.
pub type SwitchEndian = byteorder::LittleEndian;
/// Alias for [`byteorder::BigEndian`], i.e. the byte order used in the Wii/Wii U games.
pub type WiiEndian = byteorder::BigEndian;

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

/// Reads a BDAT file from a [`std::io::Read`] implementation. That type must also implement
/// [`std::io::Seek`].
///
/// This function will only read the file header. To parse tables, call [`FileReader::get_tables`].
///
/// The BDAT file format is not recommended for streams, so it is best to read from a file or a
/// byte buffer.
///
/// ```
/// use std::fs::File;
/// use bdat::{BdatResult, SwitchEndian};
///
/// fn read_file(name: &str) -> BdatResult<()> {
///     let file = File::open(name)?;
///     let file = bdat::from_reader::<_, SwitchEndian>(file)?;
///     Ok(())
/// }
/// ```
pub fn from_reader<R: Read + Seek, E: ByteOrder>(
    reader: R,
) -> Result<FileReader<BdatReader<R, E>, E>> {
    FileReader::read_file(BdatReader::new(reader))
}

/// Reads a BDAT file from a slice. The slice needs to have the **full** file data, though any
/// unrelated bytes at the end will be ignored.
///
/// This function will only read the file header. To parse tables, call [`FileReader::get_tables`].
///
/// ```
/// use std::fs::File;
/// use bdat::{BdatResult, SwitchEndian};
///
/// fn read(data: &[u8]) -> BdatResult<()> {
///     let file = bdat::from_bytes::<SwitchEndian>(data)?;
///     Ok(())
/// }
/// ```
pub fn from_bytes<E: ByteOrder>(bytes: &[u8]) -> Result<FileReader<BdatSlice<'_, E>, E>> {
    FileReader::read_file(BdatSlice::new(bytes))
}

/// Writes BDAT tables to a [`std::io::Write`] implementation that also implements [`std::io::Seek`].
///
/// ```
/// use std::fs::File;
/// use bdat::{BdatResult, BdatVersion, Table, SwitchEndian};
///
/// fn write_file(name: &str, tables: &[Table]) -> BdatResult<()> {
///     let file = File::create(name)?;
///     bdat::to_writer::<_, SwitchEndian>(file, BdatVersion::Modern, tables)?;
///     Ok(())
/// }
/// ```
pub fn to_writer<'t, W: Write + Seek, E: ByteOrder>(
    writer: W,
    version: BdatVersion,
    tables: impl IntoIterator<Item = impl Borrow<Table<'t>>>,
) -> Result<()> {
    let mut writer = BdatWriter::<W, E>::new(writer, version);
    writer.write_file(tables)
}

/// Writes BDAT tables to a `Vec<u8>`.
///
/// ```
/// use std::fs::File;
/// use bdat::{BdatResult, BdatVersion, Table, SwitchEndian};
///
/// fn write_vec(tables: &[Table]) -> BdatResult<()> {
///     let vec = bdat::to_vec::<SwitchEndian>(BdatVersion::Modern, tables)?;
///     Ok(())
/// }
/// ```
pub fn to_vec<'t, E: ByteOrder>(
    version: BdatVersion,
    tables: impl IntoIterator<Item = impl Borrow<Table<'t>>>,
) -> Result<Vec<u8>> {
    let mut vec = Vec::new();
    to_writer::<_, E>(Cursor::new(&mut vec), version, tables)?;
    Ok(vec)
}

impl BdatVersion {
    /// Gets whether the version forces labels to be hashed.
    pub fn are_labels_hashed(&self) -> bool {
        *self == BdatVersion::Modern
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        types::{Cell, ColumnDef, Label, Row, Value, ValueType},
        TableBuilder,
    };

    use super::*;

    #[test]
    fn table_write_back() {
        let table = TableBuilder::new()
            .set_name(Some(Label::Hash(0xca_fe_ba_be)))
            .add_column(ColumnDef::new(
                ValueType::HashRef,
                Label::Hash(0xde_ad_be_ef),
            ))
            .add_column(ColumnDef {
                ty: ValueType::UnsignedInt,
                label: Label::Hash(0xca_fe_ca_fe),
                offset: 4,
            })
            .add_row(Row::new(
                1,
                vec![
                    Cell::Single(Value::HashRef(0x00_00_00_01)),
                    Cell::Single(Value::UnsignedInt(10)),
                ],
            ))
            .add_row(Row::new(
                2,
                vec![
                    Cell::Single(Value::HashRef(0x01_00_00_01)),
                    Cell::Single(Value::UnsignedInt(100)),
                ],
            ))
            .build();

        let written = to_vec::<SwitchEndian>(BdatVersion::Modern, [&table]).unwrap();
        let read_back = &from_bytes::<SwitchEndian>(&written)
            .unwrap()
            .get_tables()
            .unwrap()[0];
        assert_eq!(table, *read_back);

        let new_written = to_vec::<SwitchEndian>(BdatVersion::Modern, [read_back]).unwrap();
        assert_eq!(written, new_written);
    }
}
