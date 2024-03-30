use anyhow::{Context, Result};
use bdat::{
    BdatFile, BdatResult, BdatVersion, CompatTable, LegacyVersion, SwitchEndian, WiiEndian,
};
use clap::{Args, ValueEnum};
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use itertools::Itertools;
use std::io::{Seek, Write};
use std::path::{Path, PathBuf};

pub mod fixed_vec;
pub mod hash;

#[derive(Clone)]
pub struct ProgressBarState {
    multi_bar: MultiProgress,
    pub master_bar: ProgressBar,
    child_style: ProgressStyle,
}

#[derive(Args)]
pub struct RayonPoolJobs {
    /// The number of jobs (or threads) to use in the conversion process.
    /// By default, this is the number of cores/threads in the system.
    #[arg(short, long)]
    jobs: Option<u16>,
}

#[derive(Clone, Copy, Ord, PartialOrd, Eq, PartialEq)]
pub enum BdatGame {
    Wii,
    Xcx,
    LegacySwitch,
    Modern,
}

impl BdatGame {
    pub fn version_default(version: BdatVersion) -> Self {
        match version {
            BdatVersion::Legacy(LegacyVersion::Wii) => Self::Wii,
            BdatVersion::Legacy(LegacyVersion::Switch) => Self::LegacySwitch,
            BdatVersion::Legacy(LegacyVersion::X) => Self::Xcx,
            BdatVersion::Modern => Self::Modern,
        }
    }

    pub fn from_bytes(self, bytes: &mut [u8]) -> BdatResult<Vec<CompatTable>> {
        Ok(match self {
            Self::Wii => bdat::legacy::from_bytes::<WiiEndian>(bytes, LegacyVersion::Wii)?
                .get_tables()?
                .into_iter()
                .map(Into::into)
                .collect(),
            Self::Xcx => bdat::legacy::from_bytes::<WiiEndian>(bytes, LegacyVersion::X)?
                .get_tables()?
                .into_iter()
                .map(Into::into)
                .collect(),
            Self::LegacySwitch => {
                bdat::legacy::from_bytes::<SwitchEndian>(bytes, LegacyVersion::Switch)?
                    .get_tables()?
                    .into_iter()
                    .map(Into::into)
                    .collect()
            }
            Self::Modern => bdat::modern::from_bytes::<SwitchEndian>(bytes)?
                .get_tables()?
                .into_iter()
                .map(Into::into)
                .collect(),
        })
    }

    pub fn to_writer<'b, W: Write + Seek>(
        self,
        writer: W,
        tables: impl IntoIterator<Item = CompatTable<'b>>,
    ) -> BdatResult<()> {
        if self == Self::Modern {
            let tables = tables
                .into_iter()
                .map(CompatTable::into_modern)
                .collect_vec();
            return bdat::modern::to_writer::<_, SwitchEndian>(writer, tables);
        }
        let tables = tables
            .into_iter()
            .map(CompatTable::into_legacy)
            .collect_vec();
        match self {
            Self::Wii => {
                bdat::legacy::to_writer::<_, WiiEndian>(writer, tables, LegacyVersion::Wii)
            }
            Self::LegacySwitch => {
                bdat::legacy::to_writer::<_, SwitchEndian>(writer, tables, LegacyVersion::Switch)
            }
            Self::Xcx => bdat::legacy::to_writer::<_, WiiEndian>(writer, tables, LegacyVersion::X),
            _ => unreachable!(),
        }
    }

    pub fn to_vec<'b, W: Write + Seek>(
        self,
        tables: impl IntoIterator<Item = CompatTable<'b>>,
    ) -> BdatResult<Vec<u8>> {
        if self == Self::Modern {
            let tables = tables
                .into_iter()
                .map(CompatTable::into_modern)
                .collect_vec();
            return bdat::modern::to_vec::<SwitchEndian>(tables);
        }
        let tables = tables
            .into_iter()
            .map(CompatTable::into_legacy)
            .collect_vec();
        match self {
            Self::Wii => bdat::legacy::to_vec::<WiiEndian>(tables, LegacyVersion::Wii),
            Self::LegacySwitch => {
                bdat::legacy::to_vec::<SwitchEndian>(tables, LegacyVersion::Switch)
            }
            Self::Xcx => bdat::legacy::to_vec::<WiiEndian>(tables, LegacyVersion::X),
            _ => unreachable!(),
        }
    }
}

impl ProgressBarState {
    pub fn new(master_name: &str, child_name: &str, total: usize) -> Self {
        let multi_bar = MultiProgress::new();
        let master_bar = multi_bar.add(
            ProgressBar::new(total as u64)
                .with_style(Self::build_progress_style(master_name, true)),
        );
        let child_style = Self::build_progress_style(child_name, false);

        Self {
            multi_bar,
            master_bar,
            child_style,
        }
    }

    pub fn add_child(&self, total: usize) -> ProgressBar {
        self.multi_bar
            .add(ProgressBar::new(total as u64).with_style(self.child_style.clone()))
    }

    pub fn remove_child(&self, child: &ProgressBar) {
        self.multi_bar.remove(child);
    }

    pub fn finish(&self) {
        self.master_bar.finish();
    }

    pub fn println<I: AsRef<str>>(&self, msg: I) -> std::io::Result<()> {
        self.multi_bar.println(msg)
    }

    fn build_progress_style(label: &str, with_time: bool) -> ProgressStyle {
        ProgressStyle::with_template(&match with_time {
            true => format!("{{spinner:.cyan}} [{{elapsed_precise:.cyan}}] {label}{{msg}}: {{human_pos}}/{{human_len}} ({{percent}}%) [{{bar:.cyan/blue}}] ETA: {{eta}}"),
            false => format!("{{spinner:.green}} {label}{{msg}}: {{human_pos}}/{{human_len}} ({{percent}}%) [{{bar}}]"),
        }).unwrap()
    }
}

impl RayonPoolJobs {
    /// Configures the Rayon thread pool based on the configured job count.
    pub fn configure(&self) -> Result<()> {
        let mut pool_builder = rayon::ThreadPoolBuilder::new();
        if let Some(jobs) = self.jobs {
            pool_builder = pool_builder.num_threads(jobs as usize);
        }
        pool_builder
            .build_global()
            .context("Could not build thread pool")
    }
}

impl ValueEnum for BdatGame {
    fn value_variants<'a>() -> &'a [Self] {
        &[Self::Wii, Self::LegacySwitch, Self::Xcx, Self::Modern]
    }

    fn to_possible_value<'a>(&self) -> Option<clap::builder::PossibleValue> {
        match self {
            Self::Wii => Some(clap::builder::PossibleValue::new("xc1")),
            Self::LegacySwitch => Some(clap::builder::PossibleValue::new("xc2de")),
            Self::Xcx => Some(clap::builder::PossibleValue::new("xcx")),
            Self::Modern => Some(clap::builder::PossibleValue::new("xc3")),
        }
    }
}

impl From<BdatGame> for BdatVersion {
    fn from(value: BdatGame) -> Self {
        match value {
            BdatGame::Wii => LegacyVersion::Wii.into(),
            BdatGame::Xcx => LegacyVersion::X.into(),
            BdatGame::LegacySwitch => LegacyVersion::Switch.into(),
            BdatGame::Modern => BdatVersion::Modern,
        }
    }
}

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
