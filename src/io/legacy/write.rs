use std::borrow::Borrow;
use std::collections::HashMap;
use std::hash::Hasher;
use std::io::{Cursor, Seek, SeekFrom, Write};
use std::marker::PhantomData;
use std::rc::Rc;

use byteorder::{ByteOrder, WriteBytesExt};

use crate::error::Result;
use crate::io::BDAT_MAGIC;
use crate::legacy::hash::HashTable;
use crate::legacy::util::{pad_2, pad_32, pad_4, pad_64};
use crate::legacy::{COLUMN_DEFINITION_SIZE, HEADER_SIZE};
use crate::{BdatVersion, Cell, ColumnDef, FlagDef, Label, Row, Table, Value, ValueType};

pub struct FileWriter<W, E> {
    writer: W,
    version: BdatVersion,
    _endianness: PhantomData<E>,
}

struct TableWriter<'a, 't, E, W> {
    table: &'a Table<'t>,
    buf: W,
    version: BdatVersion,
    names: StringTable,
    strings: StringTable,
    columns: Option<ColumnTables>,
    // TODO maybe regroup into header struct
    hash_table_offset: usize,
    row_data_offset: usize,
    final_padding: usize,
    _endianness: PhantomData<E>,
}

struct RowWriter<'a, 'b, 't, W, E> {
    row: &'a Row<'t>,
    columns: &'a [ColumnDef],
    writer: W,
    strings: &'b mut StringTable,
    version: BdatVersion,
    _endianness: PhantomData<E>,
}

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
    parent: usize,
    name_ptr: usize,
    name: Label,
}

struct ColumnTables {
    infos: Vec<ColumnInfo>,
    definitions: Vec<ColumnDefinition>,
    name_table: HashTable,
    info_len: usize,
    row_data_len: usize,
}

struct StringTable {
    table: Vec<Rc<str>>,
    offsets: HashMap<Rc<str>, usize>,
    base_offset: usize,
    len: usize,
}

impl<W: Write + Seek, E: ByteOrder> FileWriter<W, E> {
    pub fn new(writer: W, version: BdatVersion) -> Self {
        Self {
            writer,
            version,
            _endianness: PhantomData,
        }
    }

    pub fn write_file<'t>(
        &mut self,
        tables: impl IntoIterator<Item = impl Borrow<Table<'t>>>,
    ) -> Result<()> {
        let (table_bytes, table_offsets, total_len, table_count) = tables
            .into_iter()
            .map(|table| {
                let mut data = vec![];
                let cursor = Cursor::new(&mut data);

                TableWriter::<E, _>::new(table.borrow(), cursor, self.version)
                    .write()
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

        self.writer.write_u32::<E>(table_count as u32)?;
        self.writer.write_u32::<E>(total_len.try_into().unwrap())?;
        let offsets = table_offsets.len();
        for offset in table_offsets {
            self.writer
                .write_u32::<E>((offset + 8 + offsets * 4).try_into().unwrap())?;
        }
        self.writer.write_all(&table_bytes)?;

        Ok(())
    }
}

impl<'a, 't, E: ByteOrder, W: Write + Seek> TableWriter<'a, 't, E, W> {
    fn new(table: &'a Table<'t>, writer: W, version: BdatVersion) -> Self {
        Self {
            table,
            buf: writer,
            version,
            names: StringTable::new(HEADER_SIZE),
            strings: StringTable::new(0),
            columns: None,
            hash_table_offset: 0,
            row_data_offset: 0,
            final_padding: 0,
            _endianness: PhantomData,
        }
    }

    fn write(mut self) -> Result<()> {
        let table_start = self.buf.stream_position()?;

        self.make_layout()?;
        // Header space
        self.buf.write_all(&[0u8; HEADER_SIZE])?;

        let columns = self.columns.as_ref().unwrap();

        columns.write_infos::<E>(&mut self.buf)?;
        self.names.write(&mut self.buf)?;
        columns.write_defs::<E>(&mut self.buf)?;

        self.hash_table_offset = self.buf.stream_position()? as usize;
        columns.name_table.write_first_level::<E>(&mut self.buf)?;

        // Can now update other levels of the hash table
        {
            let pos = self.buf.stream_position()?;
            columns
                .name_table
                .write_other_levels::<E, _>(&mut self.buf)?;
            self.buf.seek(SeekFrom::Start(pos))?;
        }

        let row_start = self.buf.stream_position()?;
        self.row_data_offset = row_start as usize;
        for row in self.table.rows() {
            RowWriter::<_, E>::new(
                self.version,
                row,
                &self.table.columns,
                &mut self.strings,
                &mut self.buf,
            )
            .write()?;
        }
        let row_size = (self.buf.stream_position()? - row_start) as usize;
        for _ in row_size..pad_32(row_size) {
            self.buf.write_u8(0)?;
        }

        self.strings.base_offset = self.buf.stream_position()? as usize;
        self.strings.write(&mut self.buf)?;

        let table_size = (self.buf.stream_position()? - table_start) as usize;
        for _ in table_size..pad_64(table_size) {
            self.buf.write_u8(0)?;
            self.final_padding += 1;
        }

        // TODO - temporary solution: rows double pass
        self.buf.seek(SeekFrom::Start(row_start))?;
        for row in self.table.rows() {
            RowWriter::<_, E>::new(
                self.version,
                row,
                &self.table.columns,
                &mut self.strings,
                &mut self.buf,
            )
            .write()?;
        }

        // Write header when we have all the necessary information
        self.buf.seek(SeekFrom::Start(0))?;
        self.write_header()?;

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
        // TODO remove try_intos by checking earlier
        let columns = self.columns.as_ref().unwrap();

        self.buf.write_all(&BDAT_MAGIC)?; // "BDAT"
        self.buf.write_u16::<E>(0)?; // Scramble type

        // Name table offset = header size + column info table size
        self.buf
            .write_u16::<E>((HEADER_SIZE + columns.info_len) as u16)?;
        // Size of each row
        self.buf
            .write_u16::<E>(columns.row_data_len.try_into().unwrap())?;
        // Hash table offset
        self.buf
            .write_u16::<E>(self.hash_table_offset.try_into().unwrap())?;
        // Hash table modulo factor - TODO
        self.buf.write_u16::<E>(61)?;
        // Row table offset
        self.buf
            .write_u16::<E>(self.row_data_offset.try_into().unwrap())?;
        // Number of rows
        self.buf
            .write_u16::<E>(self.table.rows.len().try_into().unwrap())?;
        // ID of the first row
        self.buf.write_u16::<E>(
            self.table
                .rows
                .first()
                .map(Row::id)
                .unwrap_or_default()
                .try_into()
                .unwrap(),
        )?;
        // UNKNOWN - asserted 2 when reading
        self.buf.write_u16::<E>(2)?;
        // Checksum - TODO
        self.buf.write_u16::<E>(0)?;
        // String table offset
        self.buf
            .write_u32::<E>(self.strings.base_offset.try_into().unwrap())?;
        // String table size, includes final table padding
        self.buf.write_u32::<E>(
            (self.strings.size_bytes() + self.final_padding)
                .try_into()
                .unwrap(),
        )?;
        // Column definition table offset
        self.buf.write_u16::<E>(
            (self.names.base_offset + self.names.size_bytes())
                .try_into()
                .unwrap(),
        )?;
        // Column count (includes flags)
        self.buf
            .write_u16::<E>(columns.definitions.len().try_into().unwrap())?;
        // Padding
        self.buf.write_all(&[0u8; HEADER_SIZE - 36])?;

        Ok(())
    }
}

impl ColumnTables {
    fn from_columns(cols: &[ColumnDef], name_table: &mut StringTable) -> Self {
        let (row_len, mut infos) = cols
            .iter()
            .fold((0, Vec::new()), |(offset, mut cols), col| {
                let info = ColumnInfo::new(col, offset);
                let next = offset + info.data_size();
                cols.push(info);
                (next, cols)
            });
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
            .map(|c| (None, &c.label))
            .chain(
                cols.iter()
                    .enumerate()
                    .flat_map(|(i, c)| c.flags().iter().map(move |f| (Some(i), &f.label))),
            )
            .enumerate()
            .map(|(i, (cell_idx, label))| ColumnDefinition {
                info_ptr: info_offsets[i],
                // For flags, this is the offset to the parent column's definition. For regular
                // cells, this is 0
                parent: cell_idx
                    .map(|i| defs_offset + i * COLUMN_DEFINITION_SIZE)
                    .unwrap_or_default(),
                // Initially, the name table base offset is just before the info table
                name_ptr: name_table.get_offset(&label.to_string_convert()) + info_table_size,
                name: label.clone(),
            })
            .collect::<Vec<_>>();

        for (info, def) in infos.iter_mut().zip(definitions.iter()) {
            if let CellHeader::Flags { parent, .. } = &mut info.cell {
                *parent = def.parent;
            }
        }

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
            row_data_len: row_len,
        }
    }

    fn write_infos<E: ByteOrder>(&self, mut writer: impl Write) -> Result<()> {
        let mut size = 0;
        for info in &self.infos {
            info.write::<E>(&mut writer)?;
            size += info.get_size();
        }
        for _ in size..self.info_len {
            writer.write_u8(0)?;
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

impl<'a, 'b, 't, W: Write, E: ByteOrder> RowWriter<'a, 'b, 't, W, E> {
    fn new(
        version: BdatVersion,
        row: &'a Row<'t>,
        columns: &'a [ColumnDef],
        strings: &'b mut StringTable,
        writer: W,
    ) -> Self {
        Self {
            version,
            row,
            columns,
            writer,
            strings,
            _endianness: PhantomData,
        }
    }

    fn write(&mut self) -> Result<()> {
        for (cell, col) in self.row.cells.iter().zip(self.columns.iter()) {
            match cell {
                Cell::Single(v) => self.write_value(v),
                Cell::List(values) => values.iter().try_for_each(|v| self.write_value(v)),
                Cell::Flags(flags) => {
                    let mut num = 0;
                    for (def, val) in col.flags().iter().zip(flags.iter()) {
                        num |= (*val << def.flag_index) & def.mask;
                    }
                    self.write_flags(num, col.value_type)
                }
            }?
        }
        Ok(())
    }

    fn write_value(&mut self, value: &Value) -> Result<()> {
        let writer = &mut self.writer;
        Ok(match value {
            Value::Unknown => panic!("tried to serialize unknown value"),
            Value::UnsignedByte(b) => writer.write_u8(*b),
            Value::UnsignedShort(s) => writer.write_u16::<E>(*s),
            Value::UnsignedInt(i) => writer.write_u32::<E>(*i),
            Value::SignedByte(b) => writer.write_i8(*b),
            Value::SignedShort(s) => writer.write_i16::<E>(*s),
            Value::SignedInt(i) => writer.write_i32::<E>(*i),
            Value::String(s) => {
                writer.write_u32::<E>(self.strings.get_offset(s).try_into().unwrap())
            }
            Value::Float(f) => {
                let mut f = *f;
                f.make_known(self.version);
                writer.write_u32::<E>(f.to_bits())
            }
            _ => panic!("unsupported value type for legacy bdats"),
        }?)
    }

    fn write_flags(&mut self, num: u32, value_type: ValueType) -> Result<()> {
        let writer = &mut self.writer;
        Ok(match value_type {
            ValueType::UnsignedByte => writer.write_u8(num as u8),
            ValueType::UnsignedShort => writer.write_u16::<E>(num as u16),
            ValueType::UnsignedInt => writer.write_u32::<E>(num),
            ValueType::SignedByte => writer.write_i8(num as i8),
            ValueType::SignedShort => writer.write_i16::<E>(num as i16),
            ValueType::SignedInt => writer.write_i32::<E>(num as i32),
            _ => panic!("invalid value type for flag"),
        }?)
    }
}

impl ColumnInfo {
    fn new(col: &ColumnDef, offset: usize) -> Self {
        let cell = if col.count > 1 {
            CellHeader::List {
                ty: col.value_type,
                offset,
                count: col.count,
            }
        } else {
            CellHeader::Value {
                ty: col.value_type,
                offset,
            }
        };
        Self { cell }
    }

    fn new_flag(flag: &FlagDef) -> Self {
        Self {
            cell: CellHeader::Flags {
                shift: flag.flag_index.try_into().unwrap(),
                mask: flag.mask,
                parent: 0xDDBA, // bad data - overwritten later
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

    fn data_size(&self) -> usize {
        match self.cell {
            CellHeader::Value { ty, .. } => Self::value_size(ty),
            CellHeader::List { ty, count, .. } => Self::value_size(ty) * count,
            CellHeader::Flags { .. } => 0,
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

    fn value_size(value_type: ValueType) -> usize {
        match value_type {
            ValueType::Unknown => 0,
            ValueType::UnsignedByte | ValueType::SignedByte => 1,
            ValueType::UnsignedShort | ValueType::SignedShort => 2,
            ValueType::UnsignedInt
            | ValueType::SignedInt
            | ValueType::String
            | ValueType::Float => 4,
            _ => panic!("unsupported value type for legacy bdats"),
        }
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
