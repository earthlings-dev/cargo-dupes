//! Report rendering: the [`Reporter`] trait, shared presentation options
//! and section vocabulary, plus the [`text`] and [`json`] reporters.

pub mod json;
#[cfg(test)]
pub(crate) mod test_support;
pub mod text;

use std::borrow::Cow;
use std::io;
use std::path::Path;

use crate::AnalysisResult;
use crate::grouper::DuplicateGroup;
use crate::grouper::DuplicationStats;

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
  pub verbose:         bool,
}

/// Duplicate group section requested from a reporter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReportSection {
  /// Top-level exact duplicates.
  Exact,
  /// Top-level near duplicates.
  Near,
  /// Sub-function exact duplicates.
  SubExact,
  /// Sub-function near duplicates.
  SubNear,
}

impl ReportSection {
  /// The section's rendered heading.
  #[must_use]
  pub const fn title(self) -> &'static str {
    match self {
      Self::Exact => "Exact Duplicates",
      Self::Near => "Near Duplicates",
      Self::SubExact => "Sub-function Exact Duplicates",
      Self::SubNear => "Sub-function Near Duplicates",
    }
  }

  /// Message rendered when the section has no groups; `None` skips the section.
  #[must_use]
  pub const fn empty_message(self) -> Option<&'static str> {
    match self {
      Self::Exact => Some("No exact duplicates found."),
      Self::Near => Some("No near duplicates found."),
      Self::SubExact | Self::SubNear => None,
    }
  }

  /// Whether member rows render a similarity score.
  #[must_use]
  pub const fn show_similarity(self) -> bool {
    matches!(self, Self::Near | Self::SubNear)
  }

  /// Whether member rows name the owning parent function.
  #[must_use]
  pub const fn show_parent(self) -> bool {
    matches!(self, Self::SubExact | Self::SubNear)
  }
}

/// Trait for reporting analysis results.
pub trait Reporter {
  /// Render the full report (stats plus every group section).
  ///
  /// # Errors
  ///
  /// Returns any error from writing to `writer`.
  fn report_full(&self, result: &AnalysisResult, writer: &mut dyn io::Write) -> io::Result<()>;

  /// Render the statistics summary only.
  ///
  /// # Errors
  ///
  /// Returns any error from writing to `writer`.
  fn report_stats(&self, stats: &DuplicationStats, writer: &mut dyn io::Write) -> io::Result<()>;

  /// Render one duplicate-group section.
  ///
  /// # Errors
  ///
  /// Returns any error from writing to `writer`.
  fn report_groups(&self, groups: &[DuplicateGroup], writer: &mut dyn io::Write, section: ReportSection) -> io::Result<()>;

  /// Render the top-level exact-duplicates section.
  ///
  /// # Errors
  ///
  /// Returns any error from writing to `writer`.
  fn report_exact(&self, groups: &[DuplicateGroup], writer: &mut dyn io::Write) -> io::Result<()> {
    self.report_groups(groups, writer, ReportSection::Exact)
  }

  /// Render the top-level near-duplicates section.
  ///
  /// # Errors
  ///
  /// Returns any error from writing to `writer`.
  fn report_near(&self, groups: &[DuplicateGroup], writer: &mut dyn io::Write) -> io::Result<()> {
    self.report_groups(groups, writer, ReportSection::Near)
  }

  /// Render the sub-function exact-duplicates section.
  ///
  /// # Errors
  ///
  /// Returns any error from writing to `writer`.
  fn report_sub_exact(&self, groups: &[DuplicateGroup], writer: &mut dyn io::Write) -> io::Result<()> {
    self.report_groups(groups, writer, ReportSection::SubExact)
  }

  /// Render the sub-function near-duplicates section.
  ///
  /// # Errors
  ///
  /// Returns any error from writing to `writer`.
  fn report_sub_near(&self, groups: &[DuplicateGroup], writer: &mut dyn io::Write) -> io::Result<()> {
    self.report_groups(groups, writer, ReportSection::SubNear)
  }
}
