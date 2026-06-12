pub mod json;
#[cfg(test)]
pub(crate) mod test_support;
pub mod text;

use std::borrow::Cow;
use std::io;
use std::path::Path;

use crate::AnalysisResult;
use crate::grouper::{DuplicateGroup, DuplicationStats};

/// Compute a display path relative to an optional base, falling back to the absolute path.
#[must_use]
pub fn display_path<'a>(base: Option<&Path>, path: &'a Path) -> Cow<'a, str> {
    if let Some(base) = base
        && let Ok(rel) = path.strip_prefix(base)
    {
        return rel.to_string_lossy();
    }
    path.to_string_lossy()
}

/// Presentation options shared by all reporters.
#[derive(Debug, Clone, Copy, Default)]
pub struct ReportOptions {
    /// Include rule-suppressed duplicate groups in the report body.
    pub show_suppressed: bool,
    /// Verbose statistics: per-rule suppression breakdown.
    pub verbose: bool,
}

/// Duplicate group section requested from a reporter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReportSection {
    Exact,
    Near,
    SubExact,
    SubNear,
}

impl ReportSection {
    #[must_use]
    pub const fn title(self) -> &'static str {
        match self {
            Self::Exact => "Exact Duplicates",
            Self::Near => "Near Duplicates",
            Self::SubExact => "Sub-function Exact Duplicates",
            Self::SubNear => "Sub-function Near Duplicates",
        }
    }

    #[must_use]
    pub const fn empty_message(self) -> Option<&'static str> {
        match self {
            Self::Exact => Some("No exact duplicates found."),
            Self::Near => Some("No near duplicates found."),
            Self::SubExact | Self::SubNear => None,
        }
    }

    #[must_use]
    pub const fn show_similarity(self) -> bool {
        matches!(self, Self::Near | Self::SubNear)
    }

    #[must_use]
    pub const fn show_parent(self) -> bool {
        matches!(self, Self::SubExact | Self::SubNear)
    }
}

/// Trait for reporting analysis results.
pub trait Reporter {
    fn report_full(&self, result: &AnalysisResult, writer: &mut dyn io::Write) -> io::Result<()>;
    fn report_stats(&self, stats: &DuplicationStats, writer: &mut dyn io::Write) -> io::Result<()>;
    fn report_groups(
        &self,
        groups: &[DuplicateGroup],
        writer: &mut dyn io::Write,
        section: ReportSection,
    ) -> io::Result<()>;

    fn report_exact(
        &self,
        groups: &[DuplicateGroup],
        writer: &mut dyn io::Write,
    ) -> io::Result<()> {
        self.report_groups(groups, writer, ReportSection::Exact)
    }

    fn report_near(&self, groups: &[DuplicateGroup], writer: &mut dyn io::Write) -> io::Result<()> {
        self.report_groups(groups, writer, ReportSection::Near)
    }

    fn report_sub_exact(
        &self,
        groups: &[DuplicateGroup],
        writer: &mut dyn io::Write,
    ) -> io::Result<()> {
        self.report_groups(groups, writer, ReportSection::SubExact)
    }

    fn report_sub_near(
        &self,
        groups: &[DuplicateGroup],
        writer: &mut dyn io::Write,
    ) -> io::Result<()> {
        self.report_groups(groups, writer, ReportSection::SubNear)
    }
}
