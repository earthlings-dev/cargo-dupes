use std::io;

use crate::AnalysisResult;
use crate::grouper::{DuplicateGroup, DuplicationStats};
use crate::output::{ReportOptions, ReportSection, Reporter, display_path};

pub struct JsonReporter {
    pub base_path: Option<std::path::PathBuf>,
    pub options: ReportOptions,
}

// jscpd:ignore-start

impl JsonReporter {
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
}

// jscpd:ignore-end

#[derive(serde::Serialize)]
struct JsonStats {
    total_code_units: usize,
    total_lines: usize,
    exact_duplicate_groups: usize,
    exact_duplicate_units: usize,
    near_duplicate_groups: usize,
    near_duplicate_units: usize,
    exact_duplicate_lines: usize,
    near_duplicate_lines: usize,
    exact_duplicate_percent: f64,
    near_duplicate_percent: f64,
    #[serde(skip_serializing_if = "is_zero")]
    sub_exact_groups: usize,
    #[serde(skip_serializing_if = "is_zero")]
    sub_exact_units: usize,
    #[serde(skip_serializing_if = "is_zero")]
    sub_near_groups: usize,
    #[serde(skip_serializing_if = "is_zero")]
    sub_near_units: usize,
    #[serde(skip_serializing_if = "is_zero")]
    token_normalized_exact_groups: usize,
    #[serde(skip_serializing_if = "is_zero")]
    token_normalized_exact_units: usize,
    #[serde(skip_serializing_if = "is_zero")]
    token_normalized_near_groups: usize,
    #[serde(skip_serializing_if = "is_zero")]
    token_normalized_near_units: usize,
    #[serde(skip_serializing_if = "is_zero")]
    token_raw_exact_groups: usize,
    #[serde(skip_serializing_if = "is_zero")]
    token_raw_exact_units: usize,
    #[serde(skip_serializing_if = "is_zero")]
    line_exact_groups: usize,
    #[serde(skip_serializing_if = "is_zero")]
    line_exact_units: usize,
    suppressed_unit_count: usize,
    suppressed_group_count: usize,
    #[serde(skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    suppressed_by_rule: std::collections::BTreeMap<String, usize>,
    #[serde(skip_serializing_if = "is_zero")]
    ignored_group_count: usize,
}

#[allow(clippy::trivially_copy_pass_by_ref)] // serde skip_serializing_if requires &T
const fn is_zero(v: &usize) -> bool {
    *v == 0
}

#[derive(serde::Serialize)]
struct JsonGroup {
    dimension: String,
    match_kind: String,
    fingerprint: String,
    similarity: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    suppressed: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    also_seen: Vec<JsonCoverageNote>,
    members: Vec<JsonMember>,
}

#[derive(serde::Serialize)]
struct JsonCoverageNote {
    dimension: String,
    match_kind: String,
    group_count: usize,
}

#[derive(serde::Serialize)]
struct JsonMember {
    name: String,
    kind: String,
    /// Content fingerprint of the member unit, for authoring resilient
    /// `.dupes-ignore.toml` entries (`member_fingerprints`).
    fingerprint: String,
    file: String,
    line_start: usize,
    line_end: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    suppressed: Option<String>,
}

#[derive(serde::Serialize)]
struct JsonReport {
    stats: JsonStats,
    groups: Vec<JsonGroup>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    suppressed_groups: Vec<JsonGroup>,
    warnings: Vec<String>,
}

impl Reporter for JsonReporter {
    fn report_full(&self, result: &AnalysisResult, writer: &mut dyn io::Write) -> io::Result<()> {
        let report = JsonReport {
            stats: Self::to_json_stats(&result.stats),
            groups: result
                .groups()
                .map(|group| self.to_json_group(group))
                .collect(),
            suppressed_groups: if self.options.show_suppressed {
                result
                    .suppressed_groups
                    .iter()
                    .map(|group| self.to_json_group(group))
                    .collect()
            } else {
                Vec::new()
            },
            warnings: result.warnings.clone(),
        };
        let json = serde_json::to_string_pretty(&report).map_err(io::Error::other)?;
        writeln!(writer, "{json}")
    }

    fn report_stats(&self, stats: &DuplicationStats, writer: &mut dyn io::Write) -> io::Result<()> {
        let json_stats = Self::to_json_stats(stats);
        let json = serde_json::to_string_pretty(&json_stats).map_err(io::Error::other)?;
        writeln!(writer, "{json}")
    }

    fn report_groups(
        &self,
        groups: &[DuplicateGroup],
        writer: &mut dyn io::Write,
        _section: ReportSection,
    ) -> io::Result<()> {
        self.write_groups(groups, writer)
    }
}

impl JsonReporter {
    fn to_json_stats(stats: &DuplicationStats) -> JsonStats {
        JsonStats {
            total_code_units: stats.total_code_units,
            total_lines: stats.total_lines,
            exact_duplicate_groups: stats.exact_duplicate_groups,
            exact_duplicate_units: stats.exact_duplicate_units,
            near_duplicate_groups: stats.near_duplicate_groups,
            near_duplicate_units: stats.near_duplicate_units,
            exact_duplicate_lines: stats.exact_duplicate_lines,
            near_duplicate_lines: stats.near_duplicate_lines,
            exact_duplicate_percent: stats.exact_duplicate_percent(),
            near_duplicate_percent: stats.near_duplicate_percent(),
            sub_exact_groups: stats.sub_exact_groups,
            sub_exact_units: stats.sub_exact_units,
            sub_near_groups: stats.sub_near_groups,
            sub_near_units: stats.sub_near_units,
            token_normalized_exact_groups: stats.token_normalized_exact_groups,
            token_normalized_exact_units: stats.token_normalized_exact_units,
            token_normalized_near_groups: stats.token_normalized_near_groups,
            token_normalized_near_units: stats.token_normalized_near_units,
            token_raw_exact_groups: stats.token_raw_exact_groups,
            token_raw_exact_units: stats.token_raw_exact_units,
            line_exact_groups: stats.line_exact_groups,
            line_exact_units: stats.line_exact_units,
            suppressed_unit_count: stats.suppressed_unit_count,
            suppressed_group_count: stats.suppressed_group_count,
            suppressed_by_rule: stats.suppressed_by_rule.clone(),
            ignored_group_count: stats.ignored_group_count,
        }
    }

    fn write_groups(
        &self,
        groups: &[DuplicateGroup],
        writer: &mut dyn io::Write,
    ) -> io::Result<()> {
        let json_groups: Vec<JsonGroup> = groups.iter().map(|g| self.to_json_group(g)).collect();
        let json = serde_json::to_string_pretty(&json_groups).map_err(io::Error::other)?;
        writeln!(writer, "{json}")
    }

    fn to_json_group(&self, group: &DuplicateGroup) -> JsonGroup {
        JsonGroup {
            dimension: group.dimension.to_string(),
            match_kind: group.match_kind.to_string(),
            fingerprint: group.fingerprint.to_hex(),
            similarity: group.similarity,
            suppressed: group.suppressed.map(|rule| rule.as_str().to_string()),
            also_seen: group
                .also_seen
                .iter()
                .map(|note| JsonCoverageNote {
                    dimension: note.dimension.to_string(),
                    match_kind: note.match_kind.to_string(),
                    group_count: note.group_count,
                })
                .collect(),
            members: group
                .members
                .iter()
                .map(|m| JsonMember {
                    name: m.name.clone(),
                    kind: m.kind.to_string(),
                    fingerprint: m.fingerprint.to_hex(),
                    file: display_path(self.base_path.as_deref(), &m.file).into_owned(),
                    line_start: m.line_start,
                    line_end: m.line_end,
                    suppressed: m.suppressed.map(|rule| rule.as_str().to_string()),
                })
                .collect(),
        }
    }
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
    fn json_report_stats() {
        let reporter = JsonReporter::new(None);
        let stats = with_duplicate_lines(stats(50, 500, 3, 8, 2, 5), 30, 20);
        let mut buf = Vec::new();
        reporter.report_stats(&stats, &mut buf).unwrap();
        let output = String::from_utf8(buf).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();
        assert_eq!(parsed["total_code_units"], 50);
        assert_eq!(parsed["exact_duplicate_groups"], 3);
        assert_eq!(parsed["exact_duplicate_lines"], 30);
        assert_eq!(parsed["near_duplicate_lines"], 20);
        assert_eq!(parsed["exact_duplicate_percent"], 6.0);
        assert_eq!(parsed["near_duplicate_percent"], 4.0);
    }

    #[test]
    fn json_report_exact_empty() {
        let reporter = JsonReporter::new(None);
        let mut buf = Vec::new();
        reporter.report_exact(&[], &mut buf).unwrap();
        let output = String::from_utf8(buf).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();
        assert!(parsed.as_array().unwrap().is_empty());
    }

    #[test]
    fn json_report_exact_with_groups() {
        let reporter = JsonReporter::new(Some(PathBuf::from("/project")));
        let group = exact_group(vec![
            make_unit("foo", "/project/src/a.rs", 10, 20),
            make_unit("bar", "/project/src/b.rs", 30, 40),
        ]);
        let mut buf = Vec::new();
        reporter.report_exact(&[group], &mut buf).unwrap();
        let output = String::from_utf8(buf).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();
        let groups = parsed.as_array().unwrap();
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0]["members"].as_array().unwrap().len(), 2);
        assert_eq!(groups[0]["similarity"], 1.0);
        assert!(groups[0]["fingerprint"].is_string());
    }

    #[test]
    fn json_report_near_with_groups() {
        let reporter = JsonReporter::new(None);
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
        let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();
        let groups = parsed.as_array().unwrap();
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0]["fingerprint"].as_str().unwrap(), fp.to_hex());
        assert_eq!(groups[0]["similarity"], 0.85);
    }

    #[test]
    fn json_is_valid() {
        let reporter = JsonReporter::new(Some(PathBuf::from("/project")));
        let group = exact_group(vec![make_unit("foo", "/project/src/a.rs", 10, 20)]);
        let mut buf = Vec::new();
        reporter.report_exact(&[group], &mut buf).unwrap();
        let output = String::from_utf8(buf).unwrap();
        // Should be valid JSON
        assert!(serde_json::from_str::<serde_json::Value>(&output).is_ok());
    }

    #[test]
    fn json_relative_paths() {
        let reporter = JsonReporter::new(Some(PathBuf::from("/home/user/project")));
        let fp = block_fingerprint();
        let group = near_group(
            fp,
            0.9,
            vec![make_unit("foo", "/home/user/project/src/main.rs", 1, 10)],
        );
        let mut buf = Vec::new();
        reporter.report_near(&[group], &mut buf).unwrap();
        let output = String::from_utf8(buf).unwrap();
        assert!(output.contains("src/main.rs"));
        assert!(!output.contains("/home/user/project"));
    }

    #[test]
    fn json_report_full_includes_groups_and_warnings() {
        let reporter = JsonReporter::new(None);
        let result = analysis_result(
            with_duplicate_lines(stats(3, 120, 1, 2, 1, 2), 18, 12),
            vec![exact_group(vec![
                make_unit("foo", "/src/a.rs", 1, 10),
                make_unit("bar", "/src/b.rs", 20, 30),
            ])],
            vec![near_group(
                block_fingerprint(),
                0.75,
                vec![
                    make_unit("process", "/src/c.rs", 40, 50),
                    make_unit("compute", "/src/d.rs", 60, 70),
                ],
            )],
            vec!["skipped unreadable file".to_string()],
        );
        let mut buf = Vec::new();
        reporter.report_full(&result, &mut buf).unwrap();
        let output = String::from_utf8(buf).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();
        assert_eq!(parsed["groups"].as_array().unwrap().len(), 2);
        assert_eq!(parsed["warnings"][0], "skipped unreadable file");
    }
}

// jscpd:ignore-end
