use std::{
    borrow::Cow,
    collections::HashMap,
    io::{Cursor, Seek, SeekFrom, Write},
    marker::PhantomData,
    rc::Rc,
};

use byteorder::{ByteOrder, WriteBytesExt};

use crate::{
    error::Result,
    types::{Cell, Label, RawTable, Row, Value},
};

use super::{BdatVersion, FileHeader};

pub(crate) struct BdatWriter<W, E> {
    stream: W,
    version: BdatVersion,
    _endianness: PhantomData<E>,
}

struct LabelTable {
    map: HashMap<Rc<Label>, u32>,
    pairs: Vec<(Rc<Label>, u32)>,
    offset: u32,
}

impl<W, E> BdatWriter<W, E>
where
    W: Write + Seek,
    E: ByteOrder,
{
    pub fn new(writer: W, version: BdatVersion) -> Self {
        Self {
            stream: writer,
            version,
            _endianness: PhantomData,
        }
    }

    pub fn write_file(&mut self, tables: impl IntoIterator<Item = RawTable>) -> Result<()> {
        let (table_bytes, table_offsets, total_len, table_count) = tables
            .into_iter()
            .map(|table| {
                let mut data = vec![];
                let cursor = Cursor::new(&mut data);

                BdatWriter::<_, E>::new(cursor, self.version)
                    .write_table(table)
                    .map(|_| data)
            })
            .try_fold(
                (Vec::new(), Vec::new(), 0, 0),
                |(mut tot_bytes, mut offsets, len, count), table_bytes| {
                    table_bytes.map(|mut bytes| {
                        let new_len = bytes.len();
                        (
                            {
                                tot_bytes.append(&mut bytes);
                                tot_bytes
                            },
                            {
                                offsets.push(len);
                                offsets
                            },
                            len + new_len,
                            count + 1,
                        )
                    })
                },
            )?;

        let header = FileHeader {
            table_count,
            table_offsets,
        };

        self.write_header(header, total_len)?;
        self.stream.write_all(&table_bytes)?;

        Ok(())
    }

    pub fn write_header(&mut self, header: FileHeader, table_data_len: usize) -> Result<()> {
        let magic_len = match self.version {
            BdatVersion::Modern => {
                self.w_u32(0x54_41_44_42)?;
                self.w_u32(0x01_00_10_04)?;
                8
            }
            BdatVersion::Legacy => 0,
        };

        let header_len = 4 + 4 + magic_len + u32::try_from(header.table_offsets.len() * 4)?;

        self.w_u32(u32::try_from(header.table_count)?)?;
        // File size
        self.w_u32(u32::try_from(table_data_len)? + header_len)?;
        for offset in header.table_offsets {
            self.w_u32(u32::try_from(offset)? + header_len)?;
        }
        Ok(())
    }

    pub fn write_table(&mut self, table: RawTable) -> Result<()> {
        match self.version {
            BdatVersion::Modern => self.write_table_v2(table),
            _ => todo!("legacy BDATs"),
        }
    }

    fn write_table_v2(&mut self, table: RawTable) -> Result<()> {
        let table_offset = self.stream.stream_position()?;

        let column_count = table.columns.len().try_into()?;
        let row_count = table.rows.len().try_into()?;
        let base_id = table
            .rows
            .iter()
            .map(Row::id)
            .min()
            .unwrap_or_default()
            .try_into()?;

        let mut primary_keys = vec![];
        let mut label_table = LabelTable::default();
        // Table name should be the first label in the table
        label_table.get(Cow::Owned(table.name.unwrap_or(Label::Hash(0))));

        // List of column definitions
        let column_table: Vec<u8> = {
            let mut data = Vec::with_capacity(table.columns.len() * (1 + 4));

            for col in &table.columns {
                data.write_u8(col.ty as u8)?;
                data.write_u16::<E>(u16::try_from(label_table.get(Cow::Borrowed(&col.label)))?)?;
            }

            data
        };

        // List of row and cell data
        let (row_table, row_len) = {
            let mut data = vec![];
            let mut row_len = 0;

            for row in table.rows {
                let mut found_primary = false;

                for cell in row.cells {
                    match cell {
                        Cell::Single(v) => {
                            if let (false, Value::HashRef(hash)) = (found_primary, &v) {
                                found_primary = true;
                                // TODO: check if ID == row.index
                                primary_keys.push((*hash, u32::try_from(row.id)?));
                            }
                            Self::write_value_v2(&mut data, v, &mut label_table)?
                        }
                        _ => panic!("flag/list cells are not supported by modern BDAT"),
                    }
                }
                if row_len == 0 {
                    row_len = data.len();
                }
            }

            (data, row_len)
        };

        // Mapping of ID hash -> row index, sorted by hash
        let primary_key_table = {
            primary_keys.sort_unstable();
            let mut buf = Vec::with_capacity(primary_keys.len() * 2);
            for (hash, i) in primary_keys {
                buf.write_u32::<E>(hash)?;
                buf.write_u32::<E>(i - base_id)?;
            }
            buf
        };

        let ser_strings_table = label_table.write::<E>()?;

        self.w_u32(0x54_41_44_42)?; // "BDAT"
        self.w_u32(0x30_04)?; // Table

        self.w_u32(column_count)?;
        self.w_u32(row_count)?;
        self.w_u32(base_id)?;
        self.w_u32(0)?; // Unknown, always zero. Our deserialization fails if this is ever made != 0

        // Build tables. Order probably doesn't matter, but we stick to the order the game uses:
        // columns, hashes, row, strings
        let mut base_offset = (self.stream.stream_position()? - table_offset) as u32 + 4 * 6;
        self.w_u32(base_offset)?; // column offset, relative to the start of the table
        base_offset += u32::try_from(column_table.len())?;
        self.w_u32(base_offset)?; // hash table offset, relative to the start of the table
        base_offset += u32::try_from(primary_key_table.len())?;
        self.w_u32(base_offset)?; // rows offset, relative to the start of the table
        base_offset += u32::try_from(row_table.len())?;
        self.w_u32(row_len.try_into()?)?; // data length of a single row
        self.w_u32(base_offset)?;
        self.w_u32(ser_strings_table.len().try_into()?)?;

        self.stream.write_all(&column_table)?;
        self.stream.write_all(&primary_key_table)?;
        self.stream.write_all(&row_table)?;
        self.stream.write_all(&ser_strings_table)?;

        Ok(())
    }

    fn write_value_v2(
        writer: &mut impl Write,
        value: Value,
        string_map: &mut LabelTable,
    ) -> std::io::Result<()> {
        match value {
            Value::Unknown => panic!("tried to serialize unknown value"),
            Value::UnsignedByte(b) | Value::Percent(b) | Value::Unknown2(b) => writer.write_u8(b),
            Value::UnsignedShort(s) | Value::Unknown3(s) => writer.write_u16::<E>(s),
            Value::UnsignedInt(i) | Value::HashRef(i) | Value::Unknown1(i) => {
                writer.write_u32::<E>(i)
            }
            Value::SignedByte(b) => writer.write_i8(b),
            Value::SignedShort(s) => writer.write_i16::<E>(s),
            Value::SignedInt(i) => writer.write_i32::<E>(i),
            Value::String(s) => writer.write_u32::<E>(string_map.get(Cow::Owned(Label::String(s)))),
            Value::Float(f) => writer.write_f32::<E>(f),
        }
    }

    #[inline(always)]
    fn w_u32(&mut self, num: u32) -> Result<()> {
        Ok(self.stream.write_u32::<E>(num)?)
    }
}

impl LabelTable {
    pub fn get(&mut self, label: Cow<Label>) -> u32 {
        let existing = self.map.get(label.as_ref());
        if let Some(existing) = existing {
            return *existing;
        }

        // Add a new label
        if self.offset == 5 {
            // Game BDATs leave the string hash at index 5 empty (it's possibly a debug name).
            // Probably doesn't matter, but we mimic that behavior nonetheless.
            self.offset += 4;
        }

        let label = Rc::new(label.into_owned());
        let offset = self.offset;
        self.map.insert(label.clone(), offset);
        self.pairs.push((label.clone(), offset));
        self.offset += match &*label {
            Label::String(s) | Label::Unhashed(s) => u32::try_from(s.len()).unwrap() + 1,
            _ => 4,
        };

        offset
    }

    pub fn write<E: ByteOrder>(self) -> std::io::Result<Vec<u8>> {
        let mut data = vec![0u8; self.offset as usize];
        let mut cursor = Cursor::new(&mut data);
        let mut written = 1;

        // TODO: if LabelTable is only used for Modern BDATs, this can be
        // left hardcoded.
        // (whether labels are hashed)
        cursor.write_u8(0)?;

        for (label, offset) in self.pairs {
            if written != offset {
                cursor.seek(SeekFrom::Current(offset as i64 - written as i64))?;
                written = offset;
            }

            match &*label {
                Label::String(s) | Label::Unhashed(s) => {
                    cursor.write_all(s.as_bytes())?;
                    cursor.write_u8(0)?;
                    written += 1 + s.len() as u32;
                }
                Label::Hash(h) => {
                    cursor.write_u32::<E>(*h)?;
                    written += 4;
                }
            }
        }

        Ok(data)
    }
}

impl Default for LabelTable {
    fn default() -> Self {
        Self {
            map: Default::default(),
            pairs: Default::default(),
            offset: 1,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::io::{Cursor, Seek};

    use byteorder::LittleEndian;

    use crate::{
        io::read::BdatReader,
        types::{Cell, ColumnDef, Label, Row, Value, ValueType},
        TableBuilder,
    };

    use super::BdatWriter;

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

        let mut written = vec![];
        let mut cursor = Cursor::new(&mut written);
        BdatWriter::<_, LittleEndian>::new(&mut cursor, crate::io::BdatVersion::Modern)
            .write_table_v2(table.clone())
            .unwrap();

        cursor.rewind().unwrap();
        let read_back =
            BdatReader::<_, LittleEndian>::new(&mut cursor, crate::io::BdatVersion::Modern)
                .read_table()
                .unwrap();

        assert_eq!(table, read_back);

        let mut new_written = vec![];
        let mut cursor = Cursor::new(&mut new_written);
        BdatWriter::<_, LittleEndian>::new(&mut cursor, crate::io::BdatVersion::Modern)
            .write_table_v2(read_back)
            .unwrap();

        assert_eq!(written, new_written);
    }
}
