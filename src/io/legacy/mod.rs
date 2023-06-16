pub mod float;
pub mod scramble;

mod read;
mod util;

use scramble::ScrambleType;
use std::ops::Range;

#[derive(Debug)]
pub struct FileHeader {
    pub table_count: usize,
    file_size: usize,
    table_offsets: Vec<usize>,
}

#[derive(Debug)]
pub struct TableHeader {
    pub scramble_type: ScrambleType,
    hashes: OffsetAndLen,
    strings: OffsetAndLen,
    offset_names: usize,
    offset_columns: usize,
    offset_rows: usize,
    column_count: usize,
    row_count: usize,
    row_len: usize,
    base_id: usize,
}

#[derive(Debug)]
struct OffsetAndLen {
    offset: usize,
    len: usize,
}

impl OffsetAndLen {
    fn max_offset(&self) -> usize {
        self.offset + self.len
    }

    fn range(&self) -> Range<usize> {
        self.offset..self.offset + self.len
    }
}

impl From<(usize, usize)> for OffsetAndLen {
    fn from((offset, len): (usize, usize)) -> Self {
        Self { offset, len }
    }
}
