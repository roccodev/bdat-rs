use crate::compat::CompatTable;
use crate::error::Result;
use crate::table::legacy::LegacyTable;
use crate::table::modern::ModernTable;
use crate::Label;
use std::collections::HashMap;
use std::io::Cursor;
use std::marker::PhantomData;

pub struct BdatReader<R, E> {
    pub(crate) stream: R,
    pub(crate) table_offset: usize,
    _endianness: PhantomData<E>,
}

#[derive(Clone)]
pub struct BdatSlice<'b, E> {
    pub(crate) data: Cursor<&'b [u8]>,
    pub(crate) table_offset: usize,
    _endianness: PhantomData<E>,
}

pub trait BdatFile<'b> {
    /// The output table type
    type TableOut;

    /// Reads all tables from the BDAT source.
    ///
    /// ## Future compatibility
    /// This function might start returning an iterator when Rust 1.75.0
    /// hits stable (specifically [this issue](https://github.com/rust-lang/rust/issues/91611)).
    fn get_tables(&mut self) -> Result<Vec<Self::TableOut>>;

    /// Returns the number of tables in the BDAT file.
    fn table_count(&self) -> usize;

    /// Reads all tables from the BDAT source, then groups them by name.
    fn get_tables_by_name(&mut self) -> Result<HashMap<Label<'b>, Self::TableOut>>
    where
        Self::TableOut: TableName<'b>,
        Self: 'b,
    {
        self.get_tables().map(|tables| {
            tables
                .into_iter()
                .map(|t| (t.name(), t)) // TODO
                .collect()
        })
    }
}

pub trait TableName<'b> {
    fn name(&self) -> Label<'b>;
}

impl<'b, E> BdatSlice<'b, E> {
    pub fn new(bytes: &'b [u8]) -> Self {
        Self {
            data: Cursor::new(bytes),
            table_offset: 0,
            _endianness: PhantomData,
        }
    }
}

impl<R, E> BdatReader<R, E> {
    pub fn new(reader: R) -> Self {
        Self {
            stream: reader,
            table_offset: 0,
            _endianness: PhantomData,
        }
    }
}

impl<'b> TableName<'b> for ModernTable<'b> {
    fn name(&self) -> Label<'b> {
        self.name.clone()
    }
}

impl<'b> TableName<'b> for LegacyTable<'b> {
    fn name(&self) -> Label<'b> {
        self.name.clone().into()
    }
}

impl<'b> TableName<'b> for CompatTable<'b> {
    fn name(&self) -> Label<'b> {
        self.name_cloned()
    }
}
