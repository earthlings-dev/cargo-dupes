use std::io;

use crate::AnalysisResult;
use crate::grouper::{DuplicateGroup, DuplicationStats, MatchKind};
use crate::output::{ReportOptions, ReportSection, Reporter, display_path};

fn format_with_commas(n: usize) -> String {
    let s = n.to_string();
    let mut result = String::with_capacity(s.len() + s.len() / 3);
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            result.push(',');
        }
        result.push(c);
    }
    result
}

pub struct TextReporter {
    /// Base path for displaying relative paths.
    pub base_path: Option<std::path::PathBuf>,
    /// Presentation options.
    pub options: ReportOptions,
}

// jscpd:ignore-start

impl TextReporter {
    #[must_use]
    pub const fn new(base_path: Option<std::path::PathBuf>) -> Self {
        Self::with_options(
            base_path,
            ReportOptions {
                show_suppressed: false,
                verbose: false,
            },
        )
    }

    #[must_use]
    pub const fn with_options(
        base_path: Option<std::path::PathBuf>,
        options: ReportOptions,
    ) -> Self {
        Self { base_path, options }
    }

    // jscpd:ignore-end

    fn write_groups(
        &self,
        groups: &[DuplicateGroup],
        writer: &mut dyn io::Write,
        title: &str,
        empty_message: Option<&str>,
        show_similarity: bool,
        show_parent: bool,
    ) -> io::Result<()> {
        if groups.is_empty() {
            if let Some(msg) = empty_message {
                writeln!(writer, "{msg}")?;
            }
            return Ok(());
        }

        writeln!(writer, "{title}")?;
        writeln!(writer, "{}", "=".repeat(title.len()))?;
        writeln!(writer)?;

        for (i, group) in groups.iter().enumerate() {
            let fp = group.fingerprint.to_hex();
            let rule = group
                .suppressed
                .map(|rule| format!(" [rule: {}]", rule.as_str()))
                .unwrap_or_default();
            if show_similarity {
                writeln!(
                    writer,
                    "Group {} (fingerprint: {}, similarity: {:.0}%, {} members):{}",
                    i + 1,
                    fp,
                    group.similarity * 100.0,
                    group.members.len(),
                    rule,
                )?;
            } else {
                writeln!(
                    writer,
                    "Group {} (fingerprint: {}, {} members):{}",
                    i + 1,
                    fp,
                    group.members.len(),
                    rule,
                )?;
            }
            for member in &group.members {
                let parent = if show_parent {
                    member
                        .parent_name
                        .as_deref()
                        .map(|p| format!(" in {p}"))
                        .unwrap_or_default()
                } else {
                    String::new()
                };
                let marker = if group.suppressed.is_none() {
                    member
                        .suppressed
                        .map(|rule| format!(" [suppressed: {}]", rule.as_str()))
                        .unwrap_or_default()
                } else {
                    String::new()
                };
                writeln!(
                    writer,
                    "  - {} ({}){} at {}:{}-{}{}",
                    member.name,
                    member.kind,
                    parent,
                    display_path(self.base_path.as_deref(), &member.file),
                    member.line_start,
                    member.line_end,
                    marker,
                )?;
            }
            if self.options.show_suppressed {
                for note in &group.also_seen {
                    writeln!(
                        writer,
                        "  also seen as: {} {} {} group(s)",
                        note.group_count, note.dimension, note.match_kind,
                    )?;
                }
            }
            writeln!(writer)?;
        }
        Ok(())
    }

    /// Write the suppression and registry accounting lines of the stats.
    fn write_suppression_stats(
        &self,
        stats: &DuplicationStats,
        writer: &mut dyn io::Write,
    ) -> io::Result<()> {
        if stats.suppressed_unit_count > 0 || stats.suppressed_group_count > 0 {
            writeln!(writer)?;
            writeln!(
                writer,
                "Suppressed: {} units, {} groups (--show-suppressed to list)",
                stats.suppressed_unit_count, stats.suppressed_group_count
            )?;
            if self.options.verbose {
                writeln!(writer, "Suppressed by rule:")?;
                for (rule, count) in &stats.suppressed_by_rule {
                    let noun = if rule.starts_with("group.") {
                        "groups"
                    } else {
                        "units"
                    };
                    writeln!(writer, "  {rule}: {count} {noun}")?;
                }
            }
        }
        if stats.ignored_group_count > 0 {
            writeln!(
                writer,
                "Ignored (registry): {} groups",
                stats.ignored_group_count
            )?;
        }
        Ok(())
    }

    /// Write the rule-suppressed groups, partitioned per dimension and match
    /// kind under `Suppressed ...` section titles.
    fn write_suppressed_sections(
        &self,
        groups: &[DuplicateGroup],
        writer: &mut dyn io::Write,
    ) -> io::Result<()> {
        use crate::code_unit::DetectionDimension;
        let sections = [
            (
                DetectionDimension::Ast,
                MatchKind::Exact,
                "Suppressed Exact Duplicates",
            ),
            (
                DetectionDimension::Ast,
                MatchKind::Near,
                "Suppressed Near Duplicates",
            ),
            (
                DetectionDimension::SubAst,
                MatchKind::Exact,
                "Suppressed Sub-function Exact Duplicates",
            ),
            (
                DetectionDimension::SubAst,
                MatchKind::Near,
                "Suppressed Sub-function Near Duplicates",
            ),
            (
                DetectionDimension::TokenNormalized,
                MatchKind::Exact,
                "Suppressed Normalized Token Exact Duplicates",
            ),
            (
                DetectionDimension::TokenNormalized,
                MatchKind::Near,
                "Suppressed Normalized Token Near Duplicates",
            ),
            (
                DetectionDimension::TokenRaw,
                MatchKind::Exact,
                "Suppressed Raw Token Exact Duplicates",
            ),
            (
                DetectionDimension::Line,
                MatchKind::Exact,
                "Suppressed Line Exact Duplicates",
            ),
        ];
        for (dimension, match_kind, title) in sections {
            let section: Vec<DuplicateGroup> = groups
                .iter()
                .filter(|group| group.dimension == dimension && group.match_kind == match_kind)
                .cloned()
                .collect();
            self.write_groups(
                &section,
                writer,
                title,
                None,
                match_kind == MatchKind::Near,
                dimension == DetectionDimension::SubAst,
            )?;
        }
        Ok(())
    }
}

impl Reporter for TextReporter {
    fn report_full(&self, result: &AnalysisResult, writer: &mut dyn io::Write) -> io::Result<()> {
        self.report_stats(&result.stats, writer)?;
        writeln!(writer)?;
        self.report_exact(&result.exact_groups, writer)?;
        if !result.near_groups.is_empty() {
            self.report_near(&result.near_groups, writer)?;
        }
        if !result.sub_exact_groups.is_empty() {
            self.report_sub_exact(&result.sub_exact_groups, writer)?;
        }
        if !result.sub_near_groups.is_empty() {
            self.report_sub_near(&result.sub_near_groups, writer)?;
        }
        self.write_groups(
            &result.token_normalized_exact_groups,
            writer,
            "Normalized Token Exact Duplicates",
            None,
            false,
            false,
        )?;
        self.write_groups(
            &result.token_normalized_near_groups,
            writer,
            "Normalized Token Near Duplicates",
            None,
            true,
            false,
        )?;
        self.write_groups(
            &result.token_raw_exact_groups,
            writer,
            "Raw Token Exact Duplicates",
            None,
            false,
            false,
        )?;
        self.write_groups(
            &result.line_exact_groups,
            writer,
            "Line Exact Duplicates",
            None,
            false,
            false,
        )?;
        if self.options.show_suppressed {
            self.write_suppressed_sections(&result.suppressed_groups, writer)?;
        }
        Ok(())
    }

    fn report_stats(&self, stats: &DuplicationStats, writer: &mut dyn io::Write) -> io::Result<()> {
        writeln!(writer, "Duplication Statistics")?;
        writeln!(writer, "=====================")?;
        writeln!(
            writer,
            "Total code units analyzed: {}",
            stats.total_code_units
        )?;
        writeln!(writer)?;
        writeln!(
            writer,
            "Exact duplicates: {} groups ({} code units)",
            stats.exact_duplicate_groups, stats.exact_duplicate_units
        )?;
        writeln!(
            writer,
            "Near duplicates:  {} groups ({} code units)",
            stats.near_duplicate_groups, stats.near_duplicate_units
        )?;
        writeln!(writer)?;
        writeln!(
            writer,
            "Duplicated lines (exact): {}",
            stats.exact_duplicate_lines
        )?;
        writeln!(
            writer,
            "Duplicated lines (near):  {}",
            stats.near_duplicate_lines
        )?;
        writeln!(
            writer,
            "Duplication: {:.1}% exact, {:.1}% near (of {} total lines)",
            stats.exact_duplicate_percent(),
            stats.near_duplicate_percent(),
            format_with_commas(stats.total_lines),
        )?;
        write_dimension_pair(
            writer,
            "Sub-function exact: ",
            "Sub-function near:  ",
            (stats.sub_exact_groups, stats.sub_exact_units),
            (stats.sub_near_groups, stats.sub_near_units),
        )?;
        write_dimension_pair(
            writer,
            "Normalized token exact: ",
            "Normalized token near:  ",
            (
                stats.token_normalized_exact_groups,
                stats.token_normalized_exact_units,
            ),
            (
                stats.token_normalized_near_groups,
                stats.token_normalized_near_units,
            ),
        )?;
        if stats.token_raw_exact_groups > 0 {
            writeln!(
                writer,
                "Raw token exact:        {} groups ({} units)",
                stats.token_raw_exact_groups, stats.token_raw_exact_units
            )?;
        }
        if stats.line_exact_groups > 0 {
            writeln!(
                writer,
                "Line exact:             {} groups ({} units)",
                stats.line_exact_groups, stats.line_exact_units
            )?;
        }
        self.write_suppression_stats(stats, writer)
    }

    fn report_groups(
        &self,
        groups: &[DuplicateGroup],
        writer: &mut dyn io::Write,
        section: ReportSection,
    ) -> io::Result<()> {
        self.write_groups(
            groups,
            writer,
            section.title(),
            section.empty_message(),
            section.show_similarity(),
            section.show_parent(),
        )
    }
}

/// Write one paired exact/near dimension stats block when either side has
/// groups; the labels carry their own alignment padding.
fn write_dimension_pair(
    writer: &mut dyn io::Write,
    exact_label: &str,
    near_label: &str,
    exact: (usize, usize),
    near: (usize, usize),
) -> io::Result<()> {
    if exact.0 == 0 && near.0 == 0 {
        return Ok(());
    }
    writeln!(writer)?;
    writeln!(
        writer,
        "{exact_label}{} groups ({} units)",
        exact.0, exact.1
    )?;
    writeln!(writer, "{near_label}{} groups ({} units)", near.0, near.1)?;
    Ok(())
}

// jscpd:ignore-start

#[cfg(test)]
mod tests {
    use super::*;
    use crate::output::test_support::{
        analysis_result, block_fingerprint, exact_group, make_unit, near_group, stats,
        with_duplicate_lines,
    };
    use std::path::PathBuf;

    #[test]
    fn text_report_stats() {
        let reporter = TextReporter::new(None);
        let stats = with_duplicate_lines(stats(100, 1000, 5, 12, 3, 8), 61, 43);
        let mut buf = Vec::new();
        reporter.report_stats(&stats, &mut buf).unwrap();
        let output = String::from_utf8(buf).unwrap();
        assert!(output.contains("100"));
        assert!(output.contains("5 groups"));
        assert!(output.contains("3 groups"));
        assert!(output.contains("Duplicated lines (exact): 61"));
        assert!(output.contains("Duplicated lines (near):  43"));
        assert!(output.contains("Duplication: 6.1% exact, 4.3% near"));
    }

    #[test]
    fn text_report_exact_empty() {
        let reporter = TextReporter::new(None);
        let mut buf = Vec::new();
        reporter.report_exact(&[], &mut buf).unwrap();
        let output = String::from_utf8(buf).unwrap();
        assert!(output.contains("No exact duplicates"));
    }

    #[test]
    fn text_report_exact_with_groups() {
        let reporter = TextReporter::new(Some(PathBuf::from("/project")));
        let group = exact_group(vec![
            make_unit("foo", "/project/src/a.rs", 10, 20),
            make_unit("bar", "/project/src/b.rs", 30, 40),
        ]);
        let mut buf = Vec::new();
        reporter.report_exact(&[group], &mut buf).unwrap();
        let output = String::from_utf8(buf).unwrap();
        assert!(output.contains("Group 1"));
        assert!(output.contains("foo"));
        assert!(output.contains("bar"));
        assert!(output.contains("src/a.rs"));
        assert!(output.contains("src/b.rs"));
    }

    #[test]
    fn text_report_near_with_groups() {
        let reporter = TextReporter::new(None);
        let fp = block_fingerprint();
        let group = near_group(
            fp,
            0.85,
            vec![
                make_unit("process", "/src/a.rs", 10, 25),
                make_unit("compute", "/src/b.rs", 30, 45),
            ],
        );
        let mut buf = Vec::new();
        reporter.report_near(&[group], &mut buf).unwrap();
        let output = String::from_utf8(buf).unwrap();
        assert!(output.contains("fingerprint:"));
        assert!(output.contains(&fp.to_hex()));
        assert!(output.contains("85%"));
        assert!(output.contains("process"));
        assert!(output.contains("compute"));
    }

    #[test]
    fn text_report_near_empty() {
        let reporter = TextReporter::new(None);
        let mut buf = Vec::new();
        reporter.report_near(&[], &mut buf).unwrap();
        let output = String::from_utf8(buf).unwrap();
        assert!(output.contains("No near duplicates"));
    }

    #[test]
    fn text_report_full_includes_stats_and_group_sections() {
        let reporter = TextReporter::new(None);
        let result = analysis_result(
            with_duplicate_lines(stats(4, 200, 1, 2, 1, 2), 20, 15),
            vec![exact_group(vec![
                make_unit("foo", "/src/a.rs", 1, 10),
                make_unit("bar", "/src/b.rs", 20, 30),
            ])],
            vec![near_group(
                block_fingerprint(),
                0.8,
                vec![
                    make_unit("process", "/src/c.rs", 40, 50),
                    make_unit("compute", "/src/d.rs", 60, 70),
                ],
            )],
            Vec::new(),
        );
        let mut buf = Vec::new();
        reporter.report_full(&result, &mut buf).unwrap();
        let output = String::from_utf8(buf).unwrap();
        assert!(output.contains("Duplication Statistics"));
        assert!(output.contains("Exact Duplicates"));
        assert!(output.contains("Near Duplicates"));
        assert!(output.contains("process"));
    }

    #[test]
    fn relative_path_stripping() {
        let base = PathBuf::from("/home/user/project");
        let result = display_path(
            Some(base.as_path()),
            std::path::Path::new("/home/user/project/src/main.rs"),
        );
        assert_eq!(result, "src/main.rs");
    }
}

// jscpd:ignore-end
