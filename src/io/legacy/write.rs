use std::collections::HashMap;
use std::io::{Seek, SeekFrom, Write};
use std::marker::PhantomData;
use std::rc::Rc;

use byteorder::{ByteOrder, WriteBytesExt};

use crate::error::Result;
use crate::legacy::hash::HashTable;
use crate::legacy::{COLUMN_DEFINITION_SIZE, HEADER_SIZE};
use crate::{ColumnDef, FlagDef, Label, Table, ValueType};

struct FileWriter<'a, 'b, 't, W> {
    tables: &'a [&'b Table<'t>],
    writer: W,
}

struct TableWriter<'a, 't, E, W> {
    table: &'a Table<'t>,
    buf: W,
    names: StringTable,
    strings: StringTable,
    columns: Option<ColumnTables>,
    _endianness: PhantomData<E>,
}

struct CellWriter {}

#[derive(Debug)]
enum CellHeader {
    Flags {
        shift: u8,
        mask: u32,
        parent: usize,
    },
    Value {
        ty: ValueType,
        offset: usize,
    },
    List {
        ty: ValueType,
        offset: usize,
        count: usize,
    },
}

#[derive(Debug)]
struct ColumnInfo {
    cell: CellHeader,
}

#[derive(Debug)]
struct ColumnDefinition {
    info_ptr: usize,
    name_ptr: usize,
    name: Label,
}

struct ColumnTables {
    infos: Vec<ColumnInfo>,
    definitions: Vec<ColumnDefinition>,
    name_table: HashTable,
    info_len: usize,
}

struct StringTable {
    table: Vec<Rc<str>>,
    offsets: HashMap<Rc<str>, usize>,
    base_offset: usize,
    len: usize,
}

impl<'a, 't, E: ByteOrder, W: Write + Seek> TableWriter<'a, 't, E, W> {
    fn new(table: &'a Table<'t>, writer: W) -> Self {
        Self {
            table,
            buf: writer,
            names: StringTable::new(HEADER_SIZE),
            strings: StringTable::new(0),
            columns: None,
            _endianness: PhantomData,
        }
    }

    fn write(&mut self) -> Result<()> {
        self.make_layout()?;
        self.write_header()?;

        let columns = self.columns.as_ref().unwrap();

        columns.write_infos::<E>(&mut self.buf)?;
        self.names.write(&mut self.buf)?;
        columns.write_defs::<E>(&mut self.buf)?;
        columns.name_table.write_first_level::<E>(&mut self.buf)?;

        // Can now update other levels of the hash table
        {
            let pos = self.buf.stream_position()?;
            columns
                .name_table
                .write_other_levels::<E, _>(&mut self.buf)?;
            self.buf.seek(SeekFrom::Start(pos))?;
        }

        // Write row data - TODO

        // Write string table - TODO

        // TODO: write other levels of the name hash table

        Ok(())
    }

    fn make_layout(&mut self) -> Result<()> {
        self.init_names();

        let columns = ColumnTables::from_columns(&self.table.columns, &mut self.names);

        self.names.base_offset += columns.info_len;
        self.columns = Some(columns);

        Ok(())
    }

    fn init_names(&mut self) {
        // Table name is the first name
        self.names.get_offset(
            &self
                .table
                .name()
                .expect("no name in legacy table")
                .to_string_convert(),
        );
        for col in self.table.columns() {
            self.names.get_offset(&col.label.to_string_convert());
        }
        for flag in self.table.columns().flat_map(|c| c.flags().iter()) {
            self.names.get_offset(&flag.label.to_string_convert());
        }
    }

    fn write_header(&mut self) -> Result<()> {
        let columns = self.columns.as_ref().unwrap();

        self.buf.write_u32::<E>(0x54_41_44_42)?; // "BDAT"
        self.buf.write_u16::<E>(0)?; // Scramble type

        // Name table offset = header size + column info table size
        self.buf
            .write_u16::<E>((HEADER_SIZE + columns.info_len) as u16)?;
        // Size of each row - TODO
        self.buf.write_u16::<E>(0)?;
        // Hash table offset = - TODO
        self.buf.write_u16::<E>(0)?;
        // Hash table modulo factor - TODO
        self.buf.write_u16::<E>(61)?;
        // Row table offset - TODO
        self.buf.write_u16::<E>(0)?;
        // Number of rows
        self.buf
            .write_u16::<E>(self.table.rows.len().try_into().unwrap())?;
        // ID of the first row - TODO
        self.buf.write_u16::<E>(0)?;
        // UNKNOWN
        self.buf.write_u16::<E>(2)?;
        // Checksum - TODO
        self.buf.write_u16::<E>(0)?;
        // String table offset - TODO
        self.buf.write_u32::<E>(0)?;
        // String table size - TODO
        self.buf.write_u32::<E>(0)?;
        // Column definition table offset - TODO
        self.buf.write_u16::<E>(0)?;
        // Column count (includes flags) - TODO
        self.buf.write_u16::<E>(0)?;
        // Padding
        self.buf.write_all(&[0u8; 64 - 36])?;

        Ok(())
    }
}

impl ColumnTables {
    fn from_columns(cols: &[ColumnDef], name_table: &mut StringTable) -> Self {
        let mut infos = cols.iter().map(ColumnInfo::new).collect::<Vec<_>>();
        infos.extend(
            cols.iter()
                .flat_map(|c| c.flags().iter())
                .map(ColumnInfo::new_flag),
        );
        let info_offset = HEADER_SIZE;
        let (info_table_size, info_offsets) =
            infos.iter().fold((0, Vec::new()), |(sz, mut vec), next| {
                vec.push(sz + info_offset);
                let size = next.get_size();
                (sz + size, vec)
            });

        let info_table_size = pad_4(info_table_size);
        let defs_offset = info_offset + name_table.size_bytes() + info_table_size;

        let definitions = cols
            .iter()
            .map(|c| &c.label)
            .chain(cols.iter().flat_map(|c| c.flags().iter().map(|f| &f.label)))
            .enumerate()
            .map(|(i, label)| ColumnDefinition {
                info_ptr: info_offsets[i],
                // Initially, the name table base offset is just before the info table
                name_ptr: name_table.get_offset(&label.to_string_convert()) + info_table_size,
                name: label.clone(),
            })
            .collect::<Vec<_>>();

        let mut hash_table = HashTable::new(61); // TODO
        for (i, def) in definitions.iter().enumerate() {
            // TODO what happens with duplicate columns?
            hash_table.insert_unique(
                &def.name.to_string_convert(),
                (defs_offset + i * COLUMN_DEFINITION_SIZE)
                    .try_into()
                    .unwrap(),
            );
        }

        Self {
            infos,
            definitions,
            name_table: hash_table,
            info_len: info_table_size,
        }
    }

    fn write_infos<E: ByteOrder>(&self, mut writer: impl Write) -> Result<()> {
        for info in &self.infos {
            info.write::<E>(&mut writer)?;
        }
        Ok(())
    }

    fn write_defs<E: ByteOrder>(&self, mut writer: impl Write) -> Result<()> {
        for info in &self.definitions {
            info.write::<E>(&mut writer)?;
        }
        Ok(())
    }
}

impl ColumnInfo {
    fn new(col: &ColumnDef) -> Self {
        let cell = if col.count > 1 {
            CellHeader::List {
                ty: col.value_type,
                offset: col.offset,
                count: col.count,
            }
        } else {
            CellHeader::Value {
                ty: col.value_type,
                offset: col.offset,
            }
        };
        Self { cell }
    }

    fn new_flag(flag: &FlagDef) -> Self {
        Self {
            cell: CellHeader::Flags {
                shift: flag.flag_index.try_into().unwrap(),
                mask: flag.mask,
                parent: 0xDDBA, // bad data - TODO
            },
        }
    }

    fn get_size(&self) -> usize {
        1 + match self.cell {
            CellHeader::Value { .. } => 1 + 2,
            CellHeader::List { .. } => 1 + 2 + 2,
            CellHeader::Flags { .. } => 1 + 4 + 2,
        }
    }

    fn write<E: ByteOrder>(&self, mut writer: impl Write) -> Result<()> {
        writer.write_u8(match self.cell {
            CellHeader::Value { .. } => 1,
            CellHeader::List { .. } => 2,
            CellHeader::Flags { .. } => 3,
        })?;
        self.cell.write::<E>(&mut writer)
    }
}

impl ColumnDefinition {
    fn write<E: ByteOrder>(&self, mut writer: impl Write) -> Result<()> {
        writer.write_u16::<E>(self.info_ptr.try_into().unwrap())?;
        writer.write_u16::<E>(0)?; // linked node, to be written later if applicable
        writer.write_u16::<E>(self.name_ptr.try_into().unwrap())?;
        Ok(())
    }
}

impl CellHeader {
    fn write<E: ByteOrder>(&self, mut writer: impl Write) -> Result<()> {
        match self {
            CellHeader::Flags {
                shift,
                mask,
                parent,
            } => {
                writer.write_u8(*shift)?;
                writer.write_u32::<E>(*mask)?;
                writer.write_u16::<E>((*parent).try_into().unwrap())?;
            }
            CellHeader::Value { ty, offset } => {
                writer.write_u8(*ty as u8)?;
                writer.write_u16::<E>((*offset).try_into().unwrap())?;
            }
            CellHeader::List { ty, offset, count } => {
                writer.write_u8(*ty as u8)?;
                writer.write_u16::<E>((*offset).try_into().unwrap())?;
                writer.write_u16::<E>((*count).try_into().unwrap())?;
            }
        }
        Ok(())
    }
}

// TODO: pad to 2
impl StringTable {
    fn new(base_offset: usize) -> Self {
        Self {
            table: vec![],
            offsets: Default::default(),
            base_offset,
            len: 0,
        }
    }

    fn get_offset(&mut self, text: &str) -> usize {
        if let Some(offset) = self.offsets.get(text) {
            return *offset + self.base_offset;
        }
        let len = text.len();
        let text: Rc<str> = Rc::from(text);
        let offset = self.len;
        self.len += pad_2(len + 1);
        self.table.push(text.clone());
        self.offsets.insert(text, offset);
        offset + self.base_offset
    }

    fn write(&self, mut writer: impl Write) -> Result<()> {
        for text in &self.table {
            let len = text.len() + 1;
            writer.write_all(text.as_bytes())?;
            writer.write_u8(0)?;
            for _ in len..pad_2(len) {
                writer.write_u8(0)?;
            }
        }
        Ok(())
    }

    fn size_bytes(&self) -> usize {
        self.len
    }
}

#[inline]
fn pad_2(len: usize) -> usize {
    len + ((2 - (len & 1)) & 1)
}

#[inline]
fn pad_4(len: usize) -> usize {
    len + ((4 - (len & 3)) & 3)
}

#[cfg(test)]
mod tests {
    use std::fs::File;

    use crate::legacy::write::TableWriter;
    use crate::{BdatFile, SwitchEndian};

    #[test]
    fn write_v1() {
        let orig = File::open("/tmp/orig.bdat").unwrap();
        let new = File::create("/tmp/new.bdat").unwrap();

        let tables = crate::from_reader(orig).unwrap().get_tables().unwrap();

        let mut writer = TableWriter::<SwitchEndian, _>::new(&tables[0], new);
        writer.write().unwrap();
    }
}
