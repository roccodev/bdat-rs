use std::{
    borrow::Cow,
    collections::HashMap,
    fs::OpenOptions,
    hash::{BuildHasher, Hasher},
    io::{BufRead, BufReader, Read, Seek, Write},
};

use bdat::{
    hash::{murmur3_with_seed, IdentityHasher, PreHashedMap},
    types::{Label, Table},
};

#[derive(Clone, Copy, Default)]
pub struct MurmurHasher(u32);

pub type MurmurHashMap<K, V> = std::collections::HashMap<K, V, MurmurHasher>;
pub type MurmurHashSet<K> = std::collections::HashSet<K, MurmurHasher>;

impl BuildHasher for MurmurHasher {
    type Hasher = Self;

    fn build_hasher(&self) -> Self::Hasher {
        *self
    }
}

impl Hasher for MurmurHasher {
    fn finish(&self) -> u64 {
        self.0 as u64
    }

    fn write(&mut self, bytes: &[u8]) {
        self.0 = murmur3_with_seed(bytes, self.0);
    }
}

pub struct HashNameTable {
    file_name_hash: u64,
    inner: PreHashedMap<u32, String>,
}

impl HashNameTable {
    pub fn empty() -> Self {
        let map = HashMap::with_hasher(IdentityHasher::default());
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

    pub fn load_from_names(reader: impl Read) -> std::io::Result<Self> {
        let reader = BufReader::new(reader);
        let (lines, bytes) =
            reader
                .lines()
                .try_fold((Vec::new(), Vec::new()), |(mut lines, mut bytes), line| {
                    let line = line?;
                    bytes.extend_from_slice(line.as_bytes());
                    lines.push(line);
                    Ok::<_, std::io::Error>((lines, bytes))
                })?;
        let hash = bdat::hash::murmur3(&bytes) as u64;

        let mut cached = OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .open(std::env::temp_dir().join("bdat-hashes.dat"))?;

        let mut saved_hash = [0u8; 8];
        if cached.read(&mut saved_hash)? == saved_hash.len() && hash.to_le_bytes() == saved_hash {
            return Self::read(BufReader::new(cached), hash);
        }

        let mut res = Self::empty();
        res.file_name_hash = hash;
        for line in lines {
            res.inner.insert(bdat::hash::murmur3_str(&line), line);
        }

        cached.rewind()?;
        res.write(&mut cached)?;

        Ok(res)
    }

    pub fn convert_all(&self, table: &mut Table) {
        if self.inner.is_empty() {
            return;
        }
        if let Some(mut label) = table.name().cloned() {
            self.convert_label(&mut label);
            table.set_name(Some(label));
        }
        for col in table.columns_mut() {
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

    pub fn convert_label(&self, label: &mut Label) {
        if let Label::Hash(hash) = label {
            *label = self.get_label(*hash);
        }
    }

    //   _____
    // < label >
    //   -----
    //          \   ^__^
    //           \  (oo)\_______
    //              (__)\       )\/\
    //                  ||----w |
    //                  ||     ||
    pub fn convert_label_cow<'moo>(&self, label: &'moo Label) -> Cow<'moo, Label> {
        match label {
            Label::Hash(h) => Cow::Owned(self.get_label(*h)),
            l => Cow::Borrowed(l),
        }
    }

    /// Writes the table into a format that can be deserialized
    /// with [`HashNameTable::read`].
    pub fn write(&self, writer: &mut impl Write) -> std::io::Result<()> {
        writer.write_all(&self.file_name_hash.to_le_bytes())?;
        writer.write_all(&self.inner.len().to_le_bytes())?;
        for (k, v) in &self.inner {
            writer.write_all(&k.to_le_bytes())?;

            let bytes = v.as_bytes();
            writer.write_all(&(bytes.len() as u16).to_le_bytes())?;
            writer.write_all(bytes)?;
        }
        Ok(())
    }
}
