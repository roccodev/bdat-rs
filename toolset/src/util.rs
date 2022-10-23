use std::path::{Path, PathBuf};

/// Calculates the greatest common denominator between the given paths.
///
/// In other words, this returns the biggest path that is shared by all
/// paths in the list.
pub fn get_common_denominator(paths: &[impl AsRef<Path>]) -> PathBuf {
    if paths.is_empty() {
        return PathBuf::new();
    }
    let mut paths = paths
        .iter()
        .map(|p| p.as_ref().iter().collect::<Vec<_>>())
        .collect::<Vec<_>>();
    let mut common = PathBuf::new();
    'outer: for (i, to_match) in paths.remove(0).into_iter().enumerate() {
        for path in &paths {
            match path.get(i) {
                None => break 'outer,
                Some(c) if *c != to_match => break 'outer,
                _ => {}
            }
        }
        common.push(to_match);
    }
    common
}

#[cfg(test)]
mod tests {
    use super::get_common_denominator;
    use std::path::Path;

    #[test]
    fn common_paths() {
        assert_eq!(
            get_common_denominator(&["/a/b/c", "/a/b/c/d", "/a/b/e"]),
            Path::new("/a/b")
        );

        assert_eq!(
            get_common_denominator(&["a/b/c", "d/e/f", "g/h/i"]),
            Path::new("")
        );

        assert_eq!(get_common_denominator(&["/a", "/a", "/a"]), Path::new("/a"));

        assert_eq!(get_common_denominator(&["/a", "/b", "/c"]), Path::new("/"));
    }
}
