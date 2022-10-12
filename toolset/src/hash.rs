use std::{
    collections::HashMap,
    fs::OpenOptions,
    hash::{BuildHasher, Hasher},
    io::{BufRead, BufReader, Read, Seek, Write},
};

use bdat::types::{Label, RawTable};

/// A [`Hasher`] implementation for pre-hashed keys.
#[derive(Clone, Copy)]
struct IdentityHasher(u64);

pub struct HashNameTable {
    file_name_hash: u64,
    inner: HashMap<u32, String, IdentityHasher>,
}

impl HashNameTable {
    pub fn empty() -> Self {
        let map = HashMap::with_hasher(IdentityHasher(0));
        Self {
            inner: map,
            file_name_hash: 0,
        }
    }

    pub fn read(mut reader: impl Read, hash: u64) -> std::io::Result<Self> {
        let mut res = Self::empty();
        res.file_name_hash = hash;

        let mut buf = [0u8; 8];
        reader.read_exact(&mut buf)?;

        for _ in 0..usize::from_le_bytes(buf) {
            reader.read_exact(&mut buf[0..4])?;
            reader.read_exact(&mut buf[4..6])?;

            let hash = u32::from_le_bytes(buf[0..4].try_into().unwrap());
            let len = u16::from_le_bytes(buf[4..6].try_into().unwrap());

            let mut string = vec![0u8; len as usize];
            reader.read_exact(&mut string)?;

            res.inner.insert(hash, String::from_utf8(string).unwrap());
        }

        Ok(res)
    }

    pub fn load_from_names(reader: impl Read, hash: u64) -> std::io::Result<Self> {
        let mut cached = OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .open(std::env::temp_dir().join("bdat-hashes.dat"))?;

        let mut saved_hash = [0u8; 8];
        if cached.read(&mut saved_hash)? == saved_hash.len() && hash.to_le_bytes() == saved_hash {
            return Self::read(BufReader::new(cached), hash);
        }

        let reader = BufReader::new(reader);
        let mut res = Self::empty();
        res.file_name_hash = hash;
        for line in reader.lines() {
            let line = line?;
            res.inner.insert(fasthash::murmur3::hash32(&line), line);
        }

        cached.rewind()?;
        res.write(&mut cached)?;

        Ok(res)
    }

    pub fn convert_all(&self, table: &mut RawTable) {
        if self.inner.is_empty() {
            return;
        }
        if let Some(label) = &mut table.name {
            self.convert_label(label);
        }
        for col in &mut table.columns {
            self.convert_label(&mut col.label);
        }
    }

    pub fn get_label(&self, hash: u32) -> Label {
        self.unhash(hash)
            .map(|s| Label::Unhashed(s.to_string()))
            .unwrap_or_else(|| Label::Hash(hash))
    }

    pub fn unhash(&self, hash: u32) -> Option<&str> {
        self.inner.get(&hash).map(|s| s.as_str())
    }

    fn convert_label(&self, label: &mut Label) {
        if let Label::Hash(hash) = label {
            *label = self.get_label(*hash);
        }
    }

    /// Writes the table into a format that can be deserialized
    /// with [`HashNameTable::read`].
    pub fn write(&self, writer: &mut impl Write) -> std::io::Result<()> {
        writer.write(&self.file_name_hash.to_le_bytes())?;
        writer.write(&self.inner.len().to_le_bytes())?;
        for (k, v) in &self.inner {
            writer.write(&k.to_le_bytes())?;

            let bytes = v.as_bytes();
            writer.write(&(bytes.len() as u16).to_le_bytes())?;
            writer.write(bytes)?;
        }
        Ok(())
    }
}

impl BuildHasher for IdentityHasher {
    type Hasher = Self;

    fn build_hasher(&self) -> Self::Hasher {
        *self
    }
}

impl Hasher for IdentityHasher {
    fn finish(&self) -> u64 {
        self.0
    }

    fn write(&mut self, bytes: &[u8]) {
        let mut int = [0u8; 4];
        int.copy_from_slice(bytes);

        let int = u32::from_le_bytes(int) as u64;
        self.0 = int | (int << 31);
    }
}
