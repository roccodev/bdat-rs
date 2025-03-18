use rayon::{
    iter::{FromParallelIterator, ParallelIterator},
    prelude::IntoParallelIterator,
};

const MAX_WARNINGS: usize = 10;

/// Utility to track warnings and error context in toolset operations
#[derive(Default)]
pub struct OpResult {
    file_name: Option<String>,
    warnings: Vec<String>,
    tmp_warnings: Vec<String>,
}

impl OpResult {
    pub fn start_file(&mut self, file_name: String) {
        self.tmp_warnings.clear();
        self.file_name = Some(file_name);
    }

    pub fn warn(&mut self, warning: String) {
        self.tmp_warnings.push(warning);
    }

    pub fn end_file(&mut self) {
        if let Some(file) = self.file_name.take() {
            self.warnings.extend(
                self.tmp_warnings
                    .drain(..)
                    .map(|warn| format!("[{file}] {warn}")),
            );
        }
    }

    pub fn print(&self) {
        if !self.warnings.is_empty() {
            eprintln!("\n");
            eprintln!(
                "Emitted {} warning{}:",
                self.warnings.len(),
                if self.warnings.len() == 1 { "" } else { "s" }
            );
            for warning in self.warnings.iter().take(MAX_WARNINGS) {
                eprintln!("- {warning}");
            }
            if self.warnings.len() > MAX_WARNINGS {
                eprintln!("... and {} more", self.warnings.len() - MAX_WARNINGS);
            }
        }
    }
}

// Merge OpResults when collecting from the parallel iterator
impl FromParallelIterator<OpResult> for OpResult {
    fn from_par_iter<I>(par_iter: I) -> Self
    where
        I: IntoParallelIterator<Item = OpResult>,
    {
        Self {
            file_name: None,
            warnings: par_iter.into_par_iter().flat_map(|r| r.warnings).collect(),
            tmp_warnings: vec![],
        }
    }
}
