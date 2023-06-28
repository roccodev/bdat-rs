use std::any::TypeId;
use std::borrow::Borrow;
use std::collections::HashMap;
use std::io::{Cursor, Seek, SeekFrom, Write};
use std::marker::PhantomData;
use std::rc::Rc;

use byteorder::{ByteOrder, WriteBytesExt};

use crate::error::Result;
use crate::io::BDAT_MAGIC;
use crate::legacy::hash::HashTable;
use crate::legacy::util::{pad_2, pad_32, pad_4, pad_64};
use crate::legacy::{
    LegacyWriteOptions, COLUMN_NODE_SIZE, COLUMN_NODE_SIZE_WII, HEADER_SIZE, HEADER_SIZE_WII,
};
use crate::{
    BdatError, BdatVersion, Cell, ColumnDef, FlagDef, Row, Table, Value, ValueType, WiiEndian,
};

/// Writes a full BDAT file to a writer.
pub struct FileWriter<W, E> {
    writer: W,
    version: BdatVersion,
    opts: LegacyWriteOptions,
    _endianness: PhantomData<E>,
}

/// Writes a single table.
struct TableWriter<'a, 't, E, W> {
    table: &'a Table<'t>,
    buf: W,
    version: BdatVersion,
    opts: LegacyWriteOptions,
    names: StringTable,
    strings: StringTable,
    columns: Option<ColumnTables>,
    // TODO maybe regroup into header struct
    hash_table_offset: usize,
    row_data_offset: usize,
    final_padding: usize,
    _endianness: PhantomData<E>,
}

/// Writes cells from a row.
struct RowWriter<'a, 'b, 't, W, E> {
    row: &'a Row<'t>,
    table: &'b mut TableWriter<'a, 't, E, W>,
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
    name: Rc<str>,
    parent: Option<usize>,
    cell: CellHeader,
}

#[derive(Debug)]
struct ColumnNode {
    info_ptr: usize,
    parent: usize,
    name_ptr: usize,
    name: Rc<str>,
}

struct ColumnTableBuilder<'a> {
    tables: ColumnTables,
    name_table: &'a mut StringTable,
    info_offsets: Vec<usize>,
    info_offset: usize,
}

struct ColumnTables {
    infos: Vec<ColumnInfo>,
    nodes: Vec<ColumnNode>,
    hash_table: HashTable,
    info_len: usize,
    row_data_len: usize,
}

#[derive(Debug)]
struct WiiColumnNode {
    info_ptr: usize,
    linked_ptr: usize,
    name: Rc<str>,
}

#[derive(Debug)]
enum StringNode {
    String(Rc<str>),
    WiiColumn(WiiColumnNode),
}

#[derive(Debug)]
struct StringTable {
    table: Vec<StringNode>,
    offsets: HashMap<Rc<str>, usize>,
    base_offset: usize,
    len: usize,
    max_len: usize,
}

impl<W: Write + Seek, E: ByteOrder + 'static> FileWriter<W, E> {
    pub fn new(writer: W, version: BdatVersion, opts: LegacyWriteOptions) -> Self {
        Self {
            writer,
            version,
            opts,
            _endianness: PhantomData,
        }
    }

    pub fn write_file<'t>(
        &mut self,
        tables: impl IntoIterator<Item = impl Borrow<Table<'t>>>,
    ) -> Result<()> {
        let tables = tables.into_iter().by_ref().collect::<Vec<_>>();
        let mut tables = tables.iter().map(|t| t.borrow()).collect::<Vec<_>>();
        // Tables must be ordered by name
        tables.sort_unstable_by_key(|t| {
            t.name
                .as_ref()
                .expect("no name in legacy table")
                .to_string_convert()
        });

        let (table_bytes, table_offsets, total_len, table_count) = tables
            .into_iter()
            .map(|table| {
                let mut data = vec![];
                let cursor = Cursor::new(&mut data);

                TableWriter::<E, _>::new(table.borrow(), cursor, self.version, self.opts)
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
        self.writer.write_u32::<E>(total_len.try_into()?)?;
        let offsets = table_offsets.len();
        for offset in table_offsets {
            self.writer
                .write_u32::<E>((offset + 8 + offsets * 4).try_into()?)?;
        }
        self.writer.write_all(&table_bytes)?;

        Ok(())
    }
}

impl<'a, 't, E: ByteOrder + 'static, W: Write + Seek> TableWriter<'a, 't, E, W> {
    fn new(
        table: &'a Table<'t>,
        writer: W,
        version: BdatVersion,
        opts: LegacyWriteOptions,
    ) -> Self {
        Self {
            table,
            buf: writer,
            version,
            opts,
            names: StringTable::new(match version {
                BdatVersion::LegacyWii => HEADER_SIZE_WII,
                _ => HEADER_SIZE,
            }),
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
        // Header space - nice workaround for a non-const (but with an upper bound) header size
        self.buf
            .write_all(&[0u8; HEADER_SIZE][..self.header_size()])?;

        let columns = self.columns.as_ref().unwrap();

        columns.write_infos::<E>(&mut self.buf)?;
        self.names.write(&mut self.buf)?;
        if self.version != BdatVersion::LegacyWii {
            columns.write_nodes::<E>(&mut self.buf)?;
        }

        self.hash_table_offset = self.buf.stream_position()? as usize;
        columns.hash_table.write_first_level::<E>(&mut self.buf)?;

        // Can now update other levels of the hash table
        {
            let pos = self.buf.stream_position()?;
            columns
                .hash_table
                .write_other_levels::<E, _>(&mut self.buf)?;
            self.buf.seek(SeekFrom::Start(pos))?;
        }

        let row_start = self.buf.stream_position()?;
        self.row_data_offset = row_start as usize;
        for row in self.table.rows() {
            RowWriter::<_, E>::new(&mut self, row).write()?;
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
            RowWriter::<_, E>::new(&mut self, row).write()?;
        }

        // Write header when we have all the necessary information
        self.buf.seek(SeekFrom::Start(0))?;
        self.write_header()?;

        Ok(())
    }

    fn make_layout(&mut self) -> Result<()> {
        self.init_names();

        let info_offset = self.header_size();

        let columns = ColumnTableBuilder::from_columns(
            &self.table.columns,
            &mut self.names,
            self.opts.hash_slots.try_into()?,
            info_offset,
        );
        let columns = match self.version {
            BdatVersion::LegacyWii => columns.build_wii()?,
            _ => columns.build_regular()?,
        };
        self.columns = Some(columns);

        Ok(())
    }

    fn init_names(&mut self) {
        // Table name is the first name
        let table_name = &self
            .table
            .name()
            .expect("no name in legacy table")
            .to_string_convert();
        self.names.make_space(table_name);
        self.names.insert(table_name);
        for col in self.table.columns() {
            self.names
                .make_space_names(&col.label.to_string_convert(), self.version);
        }
        for flag in self.table.columns().flat_map(|c| c.flags().iter()) {
            self.names.make_space_names(&flag.label, self.version);
        }
    }

    fn write_header(&mut self) -> Result<()> {
        let columns = self.columns.as_ref().unwrap();

        self.buf.write_all(&BDAT_MAGIC)?; // "BDAT"

        let mut flags = 0;
        if TypeId::of::<E>() == TypeId::of::<WiiEndian>() {
            flags |= 0b1;
        }
        // TODO scrambled flag
        self.buf.write_all(&[flags, 0])?; // Flags

        // Name table offset = header size + column info table size
        self.buf
            .write_u16::<E>((self.header_size() + columns.info_len) as u16)?;
        // Size of each row
        self.buf.write_u16::<E>(columns.row_data_len.try_into()?)?;
        // Hash table offset
        self.buf
            .write_u16::<E>(self.hash_table_offset.try_into()?)?;
        // Hash table modulo factor
        self.buf.write_u16::<E>(self.opts.hash_slots.try_into()?)?;
        // Row table offset
        self.buf.write_u16::<E>(self.row_data_offset.try_into()?)?;
        // Number of rows
        self.buf.write_u16::<E>(self.table.rows.len().try_into()?)?;
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
            .write_u32::<E>(self.strings.base_offset.try_into()?)?;
        // String table size, includes final table padding
        self.buf
            .write_u32::<E>((self.strings.size_bytes_current() + self.final_padding).try_into()?)?;

        if self.version != BdatVersion::LegacyWii {
            // Column node table offset
            self.buf.write_u16::<E>(
                (self.names.base_offset + self.names.size_bytes_current()).try_into()?,
            )?;
            // Column count (includes flags)
            self.buf.write_u16::<E>(columns.nodes.len().try_into()?)?;
            // Padding
            self.buf.write_all(&[0u8; HEADER_SIZE - 36])?;
        }

        Ok(())
    }

    const fn header_size(&self) -> usize {
        match self.version {
            BdatVersion::LegacyWii => HEADER_SIZE_WII,
            _ => HEADER_SIZE,
        }
    }
}

impl<'a> ColumnTableBuilder<'a> {
    fn from_columns(
        cols: &[ColumnDef],
        name_table: &'a mut StringTable,
        hash_slots: u32,
        info_offset: usize,
    ) -> Self {
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
                .enumerate()
                .flat_map(|(i, c)| c.flags().iter().map(move |c| (i, c)))
                .map(|(parent, f)| ColumnInfo::new_flag(f, parent)),
        );
        let (info_table_size, info_offsets) =
            infos.iter().fold((0, Vec::new()), |(sz, mut vec), next| {
                vec.push(sz + info_offset);
                let size = next.get_size();
                (sz + size, vec)
            });

        let info_table_size = pad_4(info_table_size);
        name_table.base_offset += info_table_size;

        Self {
            info_offset,
            tables: ColumnTables {
                infos,
                nodes: Vec::new(),
                hash_table: HashTable::new(hash_slots),
                info_len: info_table_size,
                row_data_len: row_len,
            },
            name_table,
            info_offsets,
        }
    }

    fn build_wii(mut self) -> Result<ColumnTables> {
        for (i, info) in self.tables.infos.iter().enumerate() {
            let node_ptr = self.name_table.insert_wii_name(WiiColumnNode {
                info_ptr: self.info_offsets[i],
                linked_ptr: 0, // written later
                name: info.name.clone(),
            });
            self.tables
                .hash_table
                .insert_unique(&info.name, node_ptr.try_into()?);
        }

        // TODO how does this work with duplicate columns?
        for info in self.tables.infos.iter_mut() {
            if let (Some(parent_id), CellHeader::Flags { parent, .. }) =
                (info.parent, &mut info.cell)
            {
                *parent = self.name_table.get_offset(parent_id + 1).unwrap();
            }
        }
        Ok(self.tables)
    }

    fn build_regular(mut self) -> Result<ColumnTables> {
        let nodes_offset =
            self.info_offset + self.name_table.size_bytes_max() + self.tables.info_len;

        let nodes = self
            .tables
            .infos
            .iter()
            .enumerate()
            .map(|(i, info)| ColumnNode {
                info_ptr: self.info_offsets[i],
                // For flags, this is the offset to the parent column's node. For regular
                // cells, this is 0
                parent: info
                    .parent
                    .map(|i| nodes_offset + i * COLUMN_NODE_SIZE)
                    .unwrap_or_default(),
                name_ptr: self.name_table.insert(&info.name),
                name: info.name.clone(),
            })
            .collect::<Vec<_>>();

        for (info, def) in self.tables.infos.iter_mut().zip(nodes.iter()) {
            if let CellHeader::Flags { parent, .. } = &mut info.cell {
                *parent = def.parent;
            }
        }

        for (i, def) in nodes.iter().enumerate() {
            // TODO what happens with duplicate columns?
            self.tables.hash_table.insert_unique(
                &def.name,
                (nodes_offset + i * COLUMN_NODE_SIZE).try_into().unwrap(),
            );
        }

        self.tables.nodes = nodes;
        Ok(self.tables)
    }
}

impl ColumnTables {
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

    /// Not to be used with Wii bdats.
    fn write_nodes<E: ByteOrder>(&self, mut writer: impl Write) -> Result<()> {
        for info in &self.nodes {
            info.write::<E>(&mut writer)?;
        }
        Ok(())
    }
}

impl<'a, 'b, 't, W: Write, E: ByteOrder> RowWriter<'a, 'b, 't, W, E> {
    fn new(table: &'b mut TableWriter<'a, 't, E, W>, row: &'a Row<'t>) -> Self {
        Self { table, row }
    }

    fn write(&mut self) -> Result<()> {
        for (cell, col) in self.row.cells.iter().zip(self.table.table.columns.iter()) {
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
        let writer = &mut self.table.buf;
        Ok(match value {
            Value::Unknown => panic!("tried to serialize unknown value"),
            Value::UnsignedByte(b) => writer.write_u8(*b),
            Value::UnsignedShort(s) => writer.write_u16::<E>(*s),
            Value::UnsignedInt(i) => writer.write_u32::<E>(*i),
            Value::SignedByte(b) => writer.write_i8(*b),
            Value::SignedShort(s) => writer.write_i16::<E>(*s),
            Value::SignedInt(i) => writer.write_i32::<E>(*i),
            Value::String(s) => writer.write_u32::<E>(self.table.strings.insert(s).try_into()?),
            Value::Float(f) => {
                let mut f = *f;
                f.make_known(self.table.version);
                writer.write_u32::<E>(f.to_bits())
            }
            t => return Err(BdatError::UnsupportedType(t.into(), self.table.version)),
        }?)
    }

    fn write_flags(&mut self, num: u32, value_type: ValueType) -> Result<()> {
        let writer = &mut self.table.buf;
        Ok(match value_type {
            ValueType::UnsignedByte => writer.write_u8(num as u8),
            ValueType::UnsignedShort => writer.write_u16::<E>(num as u16),
            ValueType::UnsignedInt => writer.write_u32::<E>(num),
            ValueType::SignedByte => writer.write_i8(num as i8),
            ValueType::SignedShort => writer.write_i16::<E>(num as i16),
            ValueType::SignedInt => writer.write_i32::<E>(num as i32),
            t => return Err(BdatError::InvalidFlagType(t)),
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
        Self {
            name: Rc::from(col.label.to_string_convert()),
            parent: None,
            cell,
        }
    }

    fn new_flag(flag: &FlagDef, parent: usize) -> Self {
        Self {
            name: Rc::from(flag.label.as_str()),
            parent: Some(parent),
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

impl ColumnNode {
    /// Not used in Wii bdats.
    fn write<E: ByteOrder>(&self, mut writer: impl Write) -> Result<()> {
        writer.write_u16::<E>(self.info_ptr.try_into()?)?;
        writer.write_u16::<E>(0)?; // linked node, to be written later if applicable
        writer.write_u16::<E>(self.name_ptr.try_into()?)?;
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
                writer.write_u16::<E>((*parent).try_into()?)?;
            }
            CellHeader::Value { ty, offset } => {
                writer.write_u8(*ty as u8)?;
                writer.write_u16::<E>((*offset).try_into()?)?;
            }
            CellHeader::List { ty, offset, count } => {
                writer.write_u8(*ty as u8)?;
                writer.write_u16::<E>((*offset).try_into()?)?;
                writer.write_u16::<E>((*count).try_into()?)?;
            }
        }
        Ok(())
    }
}

impl StringTable {
    fn new(base_offset: usize) -> Self {
        Self {
            table: vec![],
            base_offset,
            offsets: Default::default(),
            len: 0,
            max_len: 0,
        }
    }

    fn make_space_names(&mut self, text: &str, version: BdatVersion) {
        self.make_space(text);
        if version == BdatVersion::LegacyWii {
            self.max_len += COLUMN_NODE_SIZE_WII;
        }
    }

    fn make_space(&mut self, text: &str) {
        self.max_len += pad_2(text.len() + 1);
    }

    fn insert(&mut self, text: &str) -> usize {
        if let Some(ptr) = self.offsets.get(text) {
            return *ptr + self.base_offset;
        }
        let len = text.len();
        let text: Rc<str> = Rc::from(text);
        let offset = self.len;
        self.len += pad_2(len + 1);
        self.table.push(StringNode::String(text.clone()));
        self.offsets.insert(text, offset);
        offset + self.base_offset
    }

    fn insert_wii_name(&mut self, node: WiiColumnNode) -> usize {
        let len = node.name.len();
        let offset = self.len;
        self.len += pad_2(len + 1) + COLUMN_NODE_SIZE_WII;
        self.offsets.insert(node.name.clone(), offset);
        self.table.push(StringNode::WiiColumn(node));
        offset + self.base_offset
    }

    fn get_offset(&self, chronological: usize) -> Option<usize> {
        self.table
            .get(chronological)
            .and_then(|entry| match entry {
                StringNode::String(s) => self.offsets.get(s),
                StringNode::WiiColumn(c) => self.offsets.get(&c.name),
            })
            .copied()
            .map(|o| o + self.base_offset)
    }

    fn write(&self, mut writer: impl Write) -> Result<()> {
        for text in &self.table {
            match text {
                StringNode::String(text) => {
                    let len = text.len() + 1;
                    writer.write_all(text.as_bytes())?;
                    writer.write_u8(0)?;
                    for _ in len..pad_2(len) {
                        writer.write_u8(0)?;
                    }
                }
                StringNode::WiiColumn(node) => {
                    writer.write_u16::<WiiEndian>(node.info_ptr.try_into()?)?;
                    writer.write_u16::<WiiEndian>(node.linked_ptr.try_into()?)?;
                    let len = node.name.len() + 1;
                    writer.write_all(node.name.as_bytes())?;
                    writer.write_u8(0)?;
                    for _ in len..pad_2(len) {
                        writer.write_u8(0)?;
                    }
                }
            }
        }
        Ok(())
    }

    fn size_bytes_current(&self) -> usize {
        self.len
    }

    fn size_bytes_max(&self) -> usize {
        self.max_len
    }
}
