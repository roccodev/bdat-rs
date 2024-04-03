use std::{fs::File, io::BufReader, path::Path};

use bdat::Label;

#[derive(Debug)]
pub struct Filter {
    hashes: Vec<u32>,
}

pub struct FilterArg(pub String);

pub trait FileFilter: Clone {
    /// This function does not fail: we only care about BDAT files, so if there is an error
    /// in parsing a BDAT file for the purpose of file type discovery, it should panic instead.
    fn filter_file(&self, path: impl AsRef<Path>, extension: Option<&str>) -> bool;
}

#[derive(Clone, Copy)]
pub struct BdatFileFilter;
#[derive(Clone, Copy)]
pub struct SchemaFileFilter;

impl Filter {
    pub fn contains(&self, label: &Label) -> bool {
        if self.hashes.is_empty() {
            return true;
        }

        let hash = match label {
            Label::Hash(h) => *h,
            Label::String(s) => Self::hash(s),
        };
        self.hashes.binary_search(&hash).is_ok()
    }

    fn hash(key: &str) -> u32 {
        bdat::hash::murmur3_str(key)
    }
}

impl FileFilter for BdatFileFilter {
    fn filter_file(&self, path: impl AsRef<Path>, extension: Option<&str>) -> bool {
        if extension.is_some_and(|e| e == "bdat") {
            // This also makes sure to throw errors if a ".bdat" file failed
            // version detection
            return true;
        }
        // Accept non-".bdat" files that actually appear to be BDAT files
        File::open(path)
            .map_err(|_| ())
            .and_then(|f| bdat::detect_file_version(BufReader::new(f)).map_err(|_| ()))
            .is_ok()
    }
}

impl FileFilter for SchemaFileFilter {
    fn filter_file(&self, _: impl AsRef<Path>, extension: Option<&str>) -> bool {
        extension.is_some_and(|e| e == "bschema")
    }
}

impl FromIterator<FilterArg> for Filter {
    fn from_iter<T: IntoIterator<Item = FilterArg>>(iter: T) -> Self {
        Self::from_iter(iter.into_iter().flat_map(|s| {
            match u32::from_str_radix(&s.0, 16) {
                Ok(n) => [Some(Label::Hash(n)), Some(s.0.into())]
                    .into_iter()
                    .flatten(),
                Err(_) => [Some(s.0.into()), None].into_iter().flatten(),
            }
        }))
    }
}

impl<'b> FromIterator<Label<'b>> for Filter {
    fn from_iter<T: IntoIterator<Item = Label<'b>>>(iter: T) -> Self {
        let mut hashes = iter
            .into_iter()
            .map(|l| match l {
                Label::Hash(h) => h,
                Label::String(s) => Self::hash(&s),
            })
            .collect::<Vec<_>>();
        hashes.sort_unstable();
        Self { hashes }
    }
}
