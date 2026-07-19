//! Language-agnostic core of the `cargo-dupes` / `code-dupes` duplicate
//! detection pipeline.
//!
//! Language analyzers implement [`analyzer::LanguageAnalyzer`] and feed
//! [`analyze`] / [`analyze_with_generic`], which normalize, fingerprint,
//! group, suppression-tag, ignore-filter, and aggregate findings into an
//! [`AnalysisResult`] rendered by the [`output`] reporters.

pub mod analyzer;
pub mod cli;
pub mod code_unit;
pub mod config;
pub mod error;
pub mod extractor;
pub mod fingerprint;
pub mod grouper;
pub mod ignore;
pub mod node;
pub mod output;
pub(crate) mod runs;
pub mod scanner;
pub mod similarity;
pub mod suppression;
pub mod text_units;

use std::collections::HashMap;
use std::collections::HashSet;
use std::path::PathBuf;

use analyzer::LanguageAnalyzer;
use code_unit::CodeUnit;
use code_unit::DetectionDimension;
use config::Config;
use fingerprint::Fingerprint;
use grouper::DuplicateGroup;
use grouper::DuplicationStats;

const PRECISE_COVERAGE_SUPPRESSION_RATIO: f64 = 0.8;
const SAME_DIMENSION_OVERLAP_SUPPRESSION_RATIO: f64 = 0.8;

/// The result of a full analysis run.
pub struct AnalysisResult {
  /// Aggregated duplication statistics.
  pub stats: DuplicationStats,
  /// Visible AST exact-duplicate groups.
  pub exact_groups: Vec<DuplicateGroup>,
  /// Visible AST near-duplicate groups.
  pub near_groups: Vec<DuplicateGroup>,
  /// Visible sub-function exact groups.
  pub sub_exact_groups: Vec<DuplicateGroup>,
  /// Visible sub-function near groups.
  pub sub_near_groups: Vec<DuplicateGroup>,
  /// Visible normalized token-window exact groups.
  pub token_normalized_exact_groups: Vec<DuplicateGroup>,
  /// Visible normalized token-window near groups.
  pub token_normalized_near_groups: Vec<DuplicateGroup>,
  /// Visible raw token-window exact groups.
  pub token_raw_exact_groups: Vec<DuplicateGroup>,
  /// Visible line-window exact groups.
  pub line_exact_groups: Vec<DuplicateGroup>,
  /// Groups hidden from the default report by suppression rules
  /// (post-ignore, all dimensions). Each carries its rule in `suppressed`.
  pub suppressed_groups: Vec<DuplicateGroup>,
  /// Non-fatal warnings gathered while loading config and reading files.
  pub warnings: Vec<String>,
  /// All group fingerprints (exact + near) before ignore filtering.
  /// Used by the cleanup command to identify stale ignore entries.
  pub all_fingerprints: HashSet<Fingerprint>,
  /// Member content fingerprints of every group before ignore filtering,
  /// one set per group. Used for member-based ignore-entry liveness: an
  /// entry whose recorded members all appear together in one of these sets
  /// still matches the analysis even if its group fingerprint drifted.
  pub all_member_fingerprint_sets: Vec<HashSet<Fingerprint>>,
}

impl AnalysisResult {
  /// Iterate over all filtered duplicate groups.
  pub fn groups(&self) -> impl Iterator<Item = &DuplicateGroup> {
    self
      .exact_groups
      .iter()
      .chain(self.near_groups.iter())
      .chain(self.sub_exact_groups.iter())
      .chain(self.sub_near_groups.iter())
      .chain(self.token_normalized_exact_groups.iter())
      .chain(self.token_normalized_near_groups.iter())
      .chain(self.token_raw_exact_groups.iter())
      .chain(self.line_exact_groups.iter())
  }

  /// Iterate over all filtered groups including rule-suppressed ones.
  pub fn groups_with_suppressed(&self) -> impl Iterator<Item = &DuplicateGroup> {
    self.groups().chain(self.suppressed_groups.iter())
  }
}

/// Run the full analysis pipeline using a language analyzer.
///
/// Reads each file, parses it via the analyzer, optionally filters test code,
/// then delegates to [`analyze_units`] for grouping, similarity, and stats.
pub fn analyze(analyzer: &dyn LanguageAnalyzer, files: &[PathBuf], config: &Config) -> error::Result<AnalysisResult> {
  analyze_with_generic(analyzer, files, files, config)
}

/// Run the full analysis pipeline with separate AST and generic text inputs.
pub fn analyze_with_generic(
  analyzer: &dyn LanguageAnalyzer,
  ast_files: &[PathBuf],
  generic_files: &[PathBuf],
  config: &Config,
) -> error::Result<AnalysisResult> {
  let analysis_config = config.analysis_config();
  let mut units = Vec::new();
  let mut explicit_sub_units = Vec::new();
  let mut warnings = config.load_warnings.clone();
  let mut test_ranges = TestRanges::default();

  for path in ast_files {
    let Some(source) = read_source_or_warn(path, &mut warnings) else {
      continue;
    };
    match analyzer.parse_file(path, &source, &analysis_config) {
      Ok(mut file_units) => {
        retain_non_test_units(analyzer, config.exclude_tests, &mut test_ranges, &mut file_units);
        units.extend(file_units);
        if config.sub_function && config.dimension_enabled(DetectionDimension::SubAst) {
          match analyzer.parse_sub_units(path, &source, &analysis_config, config.min_sub_nodes) {
            Ok(mut file_sub_units) => {
              retain_non_test_units(analyzer, config.exclude_tests, &mut test_ranges, &mut file_sub_units);
              explicit_sub_units.extend(file_sub_units);
            }
            Err(e) => warnings.push(e.to_string()),
          }
        }
      }
      Err(e) => warnings.push(e.to_string()),
    }
  }

  let mut token_normalized_units = Vec::new();
  let mut token_raw_units = Vec::new();
  let mut line_units = Vec::new();

  for path in generic_files {
    let Some(source) = read_source_or_warn(path, &mut warnings) else {
      continue;
    };
    let mut generic = text_units::extract(path, &source, config);
    if config.exclude_tests {
      test_ranges.retain_non_test(&mut generic.normalized_tokens);
      test_ranges.retain_non_test(&mut generic.raw_tokens);
      test_ranges.retain_non_test(&mut generic.lines);
    }
    token_normalized_units.extend(generic.normalized_tokens);
    token_raw_units.extend(generic.raw_tokens);
    line_units.extend(generic.lines);
  }

  analyze_units_with_generic(
    &units, &explicit_sub_units, &token_normalized_units, &token_raw_units, &line_units, warnings, config,
  )
}

/// Read a source file, recording a warning and returning `None` on failure.
fn read_source_or_warn(path: &std::path::Path, warnings: &mut Vec<String>) -> Option<String> {
  match std::fs::read_to_string(path) {
    Ok(source) => Some(source),
    Err(error) => {
      warnings.push(format!("Failed to read {}: {}", path.display(), error));
      None
    }
  }
}

/// Record test units in `test_ranges` and drop them when exclusion is on.
fn retain_non_test_units(analyzer: &dyn LanguageAnalyzer, exclude_tests: bool, test_ranges: &mut TestRanges, units: &mut Vec<CodeUnit>) {
  if exclude_tests {
    test_ranges.add_units(analyzer, units);
    units.retain(|unit| !analyzer.is_test_code(unit));
  }
}

/// Append a unit's line range to its file's range list.
fn push_unit_range(map: &mut HashMap<PathBuf, Vec<LineRange>>, unit: &CodeUnit) {
  map.entry(unit.file.clone()).or_default().push(LineRange {
    start: unit.line_start,
    end:   unit.line_end,
  });
}

#[derive(Default)]
struct TestRanges {
  by_file: HashMap<PathBuf, Vec<LineRange>>,
}

#[derive(Clone, Copy)]
struct LineRange {
  start: usize,
  end:   usize,
}

impl LineRange {
  const fn len(self) -> usize {
    self.end.saturating_sub(self.start) + 1
  }

  const fn intersection(self, other: Self) -> Option<Self> {
    let start = if self.start > other.start {
      self.start
    } else {
      other.start
    };
    let end = if self.end < other.end { self.end } else { other.end };
    if start <= end {
      Some(Self {
        start,
        end,
      })
    } else {
      None
    }
  }
}

impl TestRanges {
  fn add_units(&mut self, analyzer: &dyn LanguageAnalyzer, units: &[CodeUnit]) {
    for unit in units {
      if analyzer.is_test_code(unit) {
        push_unit_range(&mut self.by_file, unit);
      }
    }
  }

  fn retain_non_test(&self, units: &mut Vec<CodeUnit>) {
    units.retain(|unit| !self.contains(unit));
  }

  fn contains(&self, unit: &CodeUnit) -> bool {
    self.by_file.get(&unit.file).is_some_and(|ranges| {
      ranges
        .iter()
        .any(|range| unit.line_start >= range.start && unit.line_end <= range.end)
    })
  }
}

/// Run the analysis pipeline on pre-parsed code units.
///
/// The caller is responsible for scanning files and parsing them into `CodeUnit`s.
/// This function handles suppression tagging, grouping, similarity detection,
/// ignore filtering, and stats.
pub fn analyze_units(units: &[CodeUnit], warnings: Vec<String>, config: &Config) -> error::Result<AnalysisResult> {
  analyze_units_with_generic(units, &[], &[], &[], &[], warnings, config)
}

/// Run the analysis pipeline on pre-parsed AST and generic units.
pub fn analyze_units_with_generic(
  units: &[CodeUnit],
  explicit_sub_units: &[CodeUnit],
  token_normalized_units: &[CodeUnit],
  token_raw_units: &[CodeUnit],
  line_units: &[CodeUnit],
  warnings: Vec<String>,
  config: &Config,
) -> error::Result<AnalysisResult> {
  // Tag top-level units centrally so every language analyzer's units get
  // uniform trivial-shape suppression.
  let mut units: Vec<CodeUnit> = units.to_vec();
  tag_top_level_units(&mut units, &config.suppression);
  let units = units.as_slice();

  let mut suppressed_groups = Vec::new();
  let mut rule_unit_counts = std::collections::BTreeMap::new();
  let ast_groups = partition_matched(compute_ast_groups(units, config), &mut suppressed_groups);
  let sub_groups_have_precise_spans = !explicit_sub_units.is_empty();
  let sub_groups = compute_sub_ast_groups(units, explicit_sub_units, config, &mut suppressed_groups, &mut rule_unit_counts);
  let duplicate_coverage = DuplicateCoverage::from_matched_groups(&ast_groups, sub_groups_have_precise_spans.then_some(&sub_groups));
  let mut coverage_notes = Vec::new();
  let mut generic_groups = compute_generic_groups(
    token_normalized_units, token_raw_units, line_units, config, &duplicate_coverage, &mut coverage_notes,
  );
  generic_groups.suppressed.append(&mut suppressed_groups);
  let (mut ast_groups, mut sub_groups) = (ast_groups, sub_groups);
  apply_coverage_notes(
    &mut ast_groups,
    sub_groups_have_precise_spans.then_some(&mut sub_groups),
    coverage_notes,
  );

  let all_fingerprints = collect_group_fingerprints(&ast_groups, &sub_groups, &generic_groups);
  let all_member_fingerprint_sets = collect_member_fingerprint_sets(&ast_groups, &sub_groups, &generic_groups);

  let ignore_file = ignore::load_ignore_file(&config.root);
  let visible_before_ignore = ast_groups.len() + sub_groups.len() + generic_groups.len() + generic_groups.suppressed.len();
  let ast_groups = filter_matched_groups(ast_groups, &ignore_file);
  let sub_groups = filter_matched_groups(sub_groups, &ignore_file);
  let generic_groups = filter_generic_groups(generic_groups, &ignore_file);
  let ignored_group_count =
    visible_before_ignore - (ast_groups.len() + sub_groups.len() + generic_groups.len() + generic_groups.suppressed.len());

  let mut stats = grouper::with_generic_stats(
    grouper::compute_stats_with_sub(units, &ast_groups.exact, &ast_groups.near, &sub_groups.exact, &sub_groups.near),
    &generic_groups.token_normalized.exact,
    &generic_groups.token_normalized.near,
    &generic_groups.token_raw_exact,
    &generic_groups.line_exact,
  );
  // Totals cover the full extracted population: suppressed units are
  // counted and reported, never silently dropped from the denominator.
  stats.ignored_group_count = ignored_group_count;
  apply_suppression_stats(
    &mut stats,
    rule_unit_counts,
    &[units, token_normalized_units, token_raw_units, line_units],
    &generic_groups.suppressed,
  );

  Ok(AnalysisResult {
    suppressed_groups: generic_groups.suppressed,
    stats,
    exact_groups: ast_groups.exact,
    near_groups: ast_groups.near,
    sub_exact_groups: sub_groups.exact,
    sub_near_groups: sub_groups.near,
    token_normalized_exact_groups: generic_groups.token_normalized.exact,
    token_normalized_near_groups: generic_groups.token_normalized.near,
    token_raw_exact_groups: generic_groups.token_raw_exact,
    line_exact_groups: generic_groups.line_exact,
    warnings,
    all_fingerprints,
    all_member_fingerprint_sets,
  })
}

/// Exact and near groups for a dimension.
#[derive(Default)]
struct MatchedGroups {
  /// Exact duplicate groups.
  exact: Vec<DuplicateGroup>,
  /// Near duplicate groups.
  near:  Vec<DuplicateGroup>,
}

impl MatchedGroups {
  const fn len(&self) -> usize {
    self.exact.len() + self.near.len()
  }
}

/// Apply aggregated `also seen as` notes to the covering AST/sub groups.
///
/// `notes` reference groups by [`DuplicateCoverage`] construction order:
/// ast exact, ast near, then (when precise spans exist) sub exact, sub near.
fn apply_coverage_notes(ast_groups: &mut MatchedGroups, sub_groups: Option<&mut MatchedGroups>, notes: Vec<CoverageNoteSource>) {
  use std::collections::BTreeMap;
  let mut counts: BTreeMap<(usize, DetectionDimension, grouper::MatchKind), usize> = BTreeMap::new();
  for note in notes {
    *counts.entry((note.coverer, note.dimension, note.match_kind)).or_default() += 1;
  }
  if counts.is_empty() {
    return;
  }
  let mut coverers: Vec<&mut DuplicateGroup> = ast_groups.exact.iter_mut().chain(ast_groups.near.iter_mut()).collect();
  if let Some(sub_groups) = sub_groups {
    coverers.extend(sub_groups.exact.iter_mut().chain(sub_groups.near.iter_mut()));
  }
  for ((coverer, dimension, match_kind), group_count) in counts {
    if let Some(group) = coverers.get_mut(coverer) {
      group.also_seen.push(grouper::CoverageNote {
        dimension,
        match_kind,
        group_count,
      });
    }
  }
}

/// Generic token and line duplicate groups.
#[derive(Default)]
struct GenericGroups {
  /// Normalized token exact and near groups.
  token_normalized: MatchedGroups,
  /// Raw-token exact groups.
  token_raw_exact:  Vec<DuplicateGroup>,
  /// Normalized-line exact groups.
  line_exact:       Vec<DuplicateGroup>,
  /// Window groups hidden by suppression rules (all generic dimensions).
  suppressed:       Vec<DuplicateGroup>,
}

impl GenericGroups {
  /// Visible group count across the generic dimensions.
  const fn len(&self) -> usize {
    self.token_normalized.len() + self.token_raw_exact.len() + self.line_exact.len()
  }
}

/// Split off groups whose members are all rule-suppressed.
///
/// A group stays visible while any member is unsuppressed (mixed groups carry
/// their tagged members); a fully suppressed group takes its first member's
/// rule as the group tag. Member order is the deterministic unit scan order,
/// so the tag choice is stable.
fn partition_suppressed(groups: Vec<DuplicateGroup>, suppressed: &mut Vec<DuplicateGroup>) -> Vec<DuplicateGroup> {
  let (hidden, visible): (Vec<_>, Vec<_>) = groups
    .into_iter()
    .partition(|group| group.members.iter().all(|member| member.suppressed.is_some()));
  suppressed.extend(hidden.into_iter().map(|mut group| {
    group.suppressed = group.members.first().and_then(|member| member.suppressed);
    group
  }));
  visible
}

/// Tag top-level units with the trivial-shape suppression rules.
///
/// Tagging is centralized here so every language analyzer's units get the
/// same treatment; analyzers emit unconditionally.
fn tag_top_level_units(units: &mut [CodeUnit], policy: &suppression::SuppressionPolicy) {
  use code_unit::CodeUnitKind;
  for unit in units {
    if unit.suppressed.is_some() {
      continue;
    }
    unit.suppressed = match unit.kind {
      CodeUnitKind::Closure => {
        let body = unit.body.children.first().unwrap_or(&unit.body);
        extractor::classify_closure_body(body, policy)
      }
      CodeUnitKind::Function | CodeUnitKind::Method | CodeUnitKind::TraitImplBlock => {
        extractor::classify_top_level_body(&unit.body, policy)
      }
      _ => None,
    };
  }
}

/// Add one tally for a rule.
fn bump(counts: &mut std::collections::BTreeMap<suppression::RuleId, usize>, rule: suppression::RuleId) {
  *counts.entry(rule).or_default() += 1;
}

/// Fill the suppression accounting fields of the stats.
///
/// Unit-level rules count tagged units across the top-level and window
/// populations plus the sub-unit tallies collected at classification time;
/// chain-covered members are counted from their suppressed groups (they are
/// tagged at the group stage). Group-level rules count suppressed groups.
fn apply_suppression_stats(
  stats: &mut DuplicationStats,
  rule_unit_counts: std::collections::BTreeMap<suppression::RuleId, usize>,
  unit_populations: &[&[CodeUnit]],
  suppressed_groups: &[DuplicateGroup],
) {
  use suppression::RuleId;
  let mut rule_unit_counts = rule_unit_counts;
  for units in unit_populations {
    for rule in units.iter().filter_map(|unit| unit.suppressed) {
      bump(&mut rule_unit_counts, rule);
    }
  }
  let mut rule_group_counts: std::collections::BTreeMap<RuleId, usize> = std::collections::BTreeMap::new();
  for group in suppressed_groups {
    match group.suppressed {
      Some(rule @ (RuleId::GroupCoveredByAst | RuleId::GroupOverlapContained)) => {
        bump(&mut rule_group_counts, rule);
      }
      Some(rule @ RuleId::SubCoveredByChain) => {
        *rule_unit_counts.entry(rule).or_default() += group.members.len();
      }
      _ => {}
    }
  }
  stats.suppressed_unit_count = rule_unit_counts.values().sum();
  stats.suppressed_group_count = suppressed_groups.len();
  stats.suppressed_by_rule = rule_unit_counts
    .into_iter()
    .chain(rule_group_counts)
    .map(|(rule, count)| (rule.as_str().to_string(), count))
    .collect();
}

/// Partition both match kinds of a dimension into visible and suppressed.
fn partition_matched(mut groups: MatchedGroups, suppressed: &mut Vec<DuplicateGroup>) -> MatchedGroups {
  groups.exact = partition_suppressed(groups.exact, suppressed);
  groups.near = partition_suppressed(groups.near, suppressed);
  groups
}

/// Compute top-level AST duplicate groups when enabled.
fn compute_ast_groups(units: &[CodeUnit], config: &Config) -> MatchedGroups {
  if config.dimension_enabled(DetectionDimension::Ast) {
    compute_matched_groups(units, config.similarity_threshold, DetectionDimension::Ast)
  } else {
    MatchedGroups::default()
  }
}

/// Compute nested AST duplicate groups when enabled.
fn compute_sub_ast_groups(
  units: &[CodeUnit],
  explicit_sub_units: &[CodeUnit],
  config: &Config,
  suppressed: &mut Vec<DuplicateGroup>,
  rule_unit_counts: &mut std::collections::BTreeMap<suppression::RuleId, usize>,
) -> MatchedGroups {
  if !(config.sub_function && config.dimension_enabled(DetectionDimension::SubAst)) {
    return MatchedGroups::default();
  }

  let mut sub_units = if explicit_sub_units.is_empty() {
    fallback_sub_units(units, config.min_sub_nodes)
  } else {
    explicit_sub_units.to_vec()
  };
  for unit in &mut sub_units {
    if unit.suppressed.is_none() {
      unit.suppressed = extractor::classify_sub_unit(&unit.body, &config.suppression);
    }
    if let Some(rule) = unit.suppressed {
      bump(rule_unit_counts, rule);
    }
  }
  let sub_units = dedupe_same_region_sub_units(sub_units);

  let mut groups = compute_matched_groups(&sub_units, config.similarity_threshold, DetectionDimension::SubAst);
  apply_chain_coverage(&mut groups, &config.suppression);
  partition_matched(groups, suppressed)
}

/// Tag branch groups whose members all belong to duplicated if-chains.
///
/// Coverage requires the owning chain to have actually grouped: branches of
/// chains with no duplicate partner are released to full visibility, so
/// cross-file branch-shape repetition (option-override clusters) stays
/// detectable even when the chains differ as wholes.
fn apply_chain_coverage(groups: &mut MatchedGroups, policy: &suppression::SuppressionPolicy) {
  if !policy.is_enabled(suppression::RuleId::SubCoveredByChain) {
    return;
  }
  let duplicated_chains: HashSet<Fingerprint> = grouper::member_fingerprints_iter(groups.exact.iter().filter(|group| {
    group
      .members
      .first()
      .is_some_and(|member| member.kind == code_unit::CodeUnitKind::IfChain)
  }))
  .collect();
  for group in groups.exact.iter_mut().chain(groups.near.iter_mut()) {
    let chain_covered = !group.members.is_empty()
      && group
        .members
        .iter()
        .all(|member| member.parent_chain.is_some_and(|chain| duplicated_chains.contains(&chain)));
    if chain_covered {
      for member in &mut group.members {
        member.suppressed = policy.allow(suppression::RuleId::SubCoveredByChain);
      }
    }
  }
}

/// Remove redundant same-region representations of identical content.
///
/// A match arm or if branch whose body is exactly an if-chain produces two
/// sub-units with the same fingerprint over essentially the same lines; only
/// the widest one may represent the region, otherwise the pair forms a
/// self-referential "duplicate group" at a single source location.
fn dedupe_same_region_sub_units(mut units: Vec<CodeUnit>) -> Vec<CodeUnit> {
  units.sort_by(|a, b| {
    a.file
      .cmp(&b.file)
      .then_with(|| a.fingerprint.cmp(&b.fingerprint))
      .then_with(|| grouper::unit_line_count(b).cmp(&grouper::unit_line_count(a)))
      .then_with(|| a.line_start.cmp(&b.line_start))
      .then_with(|| a.name.cmp(&b.name))
  });
  let mut kept: Vec<CodeUnit> = Vec::new();
  for unit in units {
    let redundant = kept
      .iter()
      .any(|existing| existing.file == unit.file && existing.fingerprint == unit.fingerprint && unit_ranges_overlap(existing, &unit));
    if !redundant {
      kept.push(unit);
    }
  }
  kept
}

/// Return true when two units' line ranges intersect.
fn unit_ranges_overlap(a: &CodeUnit, b: &CodeUnit) -> bool {
  a.line_start.max(b.line_start) <= a.line_end.min(b.line_end)
}

/// Compute token and line duplicate groups when their dimensions are enabled.
///
/// Cross-dimension coverage events are appended to `notes` so the covering
/// AST/sub groups can carry `also seen as` annotations.
fn compute_generic_groups(
  token_normalized_units: &[CodeUnit],
  token_raw_units: &[CodeUnit],
  line_units: &[CodeUnit],
  config: &Config,
  duplicate_coverage: &DuplicateCoverage,
  notes: &mut Vec<CoverageNoteSource>,
) -> GenericGroups {
  let policy = &config.suppression;
  let mut suppressed = Vec::new();

  let mut token_normalized = if config.dimension_enabled(DetectionDimension::TokenNormalized) {
    compute_matched_groups(
      token_normalized_units,
      config.token_similarity_threshold,
      DetectionDimension::TokenNormalized,
    )
  } else {
    MatchedGroups::default()
  };
  token_normalized.exact = partition_suppressed(token_normalized.exact, &mut suppressed);
  token_normalized.near = partition_suppressed(token_normalized.near, &mut suppressed);
  token_normalized.exact = suppress_overlapping_groups(token_normalized.exact, policy, &mut suppressed);
  token_normalized.near = suppress_overlapping_groups(token_normalized.near, policy, &mut suppressed);
  token_normalized.exact = duplicate_coverage.split_substantially_covered(token_normalized.exact, policy, &mut suppressed, notes);
  token_normalized.near = duplicate_coverage.split_substantially_covered(token_normalized.near, policy, &mut suppressed, notes);

  let token_raw_exact = if config.dimension_enabled(DetectionDimension::TokenRaw) {
    grouper::group_exact_duplicates_for(token_raw_units, DetectionDimension::TokenRaw)
  } else {
    Vec::new()
  };
  let token_raw_exact = partition_suppressed(token_raw_exact, &mut suppressed);
  let token_raw_exact = suppress_overlapping_groups(token_raw_exact, policy, &mut suppressed);
  let token_raw_exact = duplicate_coverage.split_substantially_covered(token_raw_exact, policy, &mut suppressed, notes);

  let line_exact = if config.dimension_enabled(DetectionDimension::Line) {
    grouper::group_exact_duplicates_for(line_units, DetectionDimension::Line)
  } else {
    Vec::new()
  };
  // Partition before merging so the visible merge sees exactly the groups
  // the pre-tagging pipeline saw; the suppressed population is merged too so
  // `--show-suppressed` reads as concept regions instead of window chains.
  let mut line_suppressed = Vec::new();
  let line_exact = partition_suppressed(line_exact, &mut line_suppressed);
  let line_exact = merge_shifted_window_groups(line_exact);
  suppressed.extend(merge_shifted_window_groups(line_suppressed));
  let line_exact = suppress_overlapping_groups(line_exact, policy, &mut suppressed);
  let line_exact = duplicate_coverage.split_substantially_covered(line_exact, policy, &mut suppressed, notes);

  GenericGroups {
    token_normalized,
    token_raw_exact,
    line_exact,
    suppressed,
  }
}

/// Merge exact window groups that are shifted continuations of one another.
///
/// Sliding extraction reports a duplicated region longer than one window as a
/// chain of overlapping groups, each shifted by a few lines. When two groups
/// pair member-for-member in the same files with one uniform shift and
/// compatible window content, they describe the same duplicated region, so
/// they are merged into a single concept-level group whose members span the
/// union range. Merged units are rebuilt from the union content, keeping the
/// group fingerprint identical to what a directly extracted window of that
/// span would produce.
fn merge_shifted_window_groups(groups: Vec<DuplicateGroup>) -> Vec<DuplicateGroup> {
  let mut merged: Vec<DuplicateGroup> = Vec::new();
  let mut pending = groups;
  for group in &mut pending {
    group
      .members
      .sort_by(|a, b| a.file.cmp(&b.file).then(a.line_start.cmp(&b.line_start)));
  }
  pending.sort_by(|a, b| {
    member_signature(a)
      .cmp(&member_signature(b))
      .then_with(|| group_start_key(a).cmp(&group_start_key(b)))
  });

  for group in pending {
    if let Some(last) = merged.last_mut()
      && let Some(combined) = merge_window_group_pair(last, &group)
    {
      *last = combined;
      continue;
    }
    merged.push(group);
  }

  merged.sort_by(grouper::compare_group_size_desc_then_fingerprint);
  merged
}

/// Files of each member, used to bucket groups that could pair up.
fn member_signature(group: &DuplicateGroup) -> Vec<&PathBuf> {
  group.members.iter().map(|member| &member.file).collect()
}

/// Earliest member position, used to order shifted groups along a region.
fn group_start_key(group: &DuplicateGroup) -> Option<(&PathBuf, usize)> {
  group.members.first().map(|member| (&member.file, member.line_start))
}

/// Try to merge `next` into `current` as a shifted continuation.
fn merge_window_group_pair(current: &DuplicateGroup, next: &DuplicateGroup) -> Option<DuplicateGroup> {
  if current.dimension != next.dimension
    || current.match_kind != grouper::MatchKind::Exact
    || next.match_kind != grouper::MatchKind::Exact
    || current.members.len() != next.members.len()
  {
    return None;
  }

  let current_values = contiguous_window_values(&current.members[0])?;
  let next_values = contiguous_window_values(&next.members[0])?;

  let first_current = &current.members[0];
  let first_next = &next.members[0];
  if first_next.line_start <= first_current.line_start {
    return None;
  }
  let shift = first_next.line_start - first_current.line_start;
  if shift > current_values.len() || first_next.line_end <= first_current.line_end {
    return None;
  }

  for (member, next_member) in current.members.iter().zip(&next.members) {
    if member.file != next_member.file
      || next_member.line_start != member.line_start + shift
      || contiguous_window_values(member).is_none()
      || contiguous_window_values(next_member).is_none()
    {
      return None;
    }
  }

  // The retained suffix of the current window must equal the overlapping
  // prefix of the next window for the union to be one duplicated region.
  let overlap = current_values.len() - shift;
  if current_values[shift..] != next_values[..overlap] {
    return None;
  }

  let union_values: Vec<String> = current_values
    .iter()
    .chain(next_values[overlap..].iter())
    .map(|value| (*value).to_string())
    .collect();

  let members: Vec<CodeUnit> = current
    .members
    .iter()
    .zip(&next.members)
    .map(|(member, next_member)| {
      text_units::window_unit(
        &member.file,
        &member.name,
        member.kind.clone(),
        member.line_start,
        next_member.line_end,
        &union_values,
      )
    })
    .collect();

  let mut members = members;
  for member in &mut members {
    member.suppressed = current.suppressed;
  }
  let content_fingerprint = members[0].fingerprint;
  Some(DuplicateGroup {
    // Merging happens within one population, so a suppressed chain keeps
    // its group tag (and member tags) through the rebuilt union members.
    suppressed: current.suppressed,
    also_seen: Vec::new(),
    dimension: current.dimension,
    match_kind: grouper::MatchKind::Exact,
    fingerprint: grouper::exact_group_fingerprint(current.dimension, content_fingerprint),
    members,
    similarity: 1.0,
  })
}

/// Window values when the unit's content lines map one-to-one to source lines.
fn contiguous_window_values(unit: &CodeUnit) -> Option<Vec<&str>> {
  let values = text_units::window_values(unit)?;
  (values.len() == grouper::unit_line_count(unit)).then_some(values)
}

/// Source ranges already explained by AST/sub-AST duplicate groups.
///
/// Coverage is tracked per duplicate group: a generic token/line group is
/// redundant only when one single stronger group covers all of its members,
/// meaning both describe the same duplicate concept. Members that merely fall
/// inside unrelated duplicate ranges scattered across different groups do not
/// suppress a generic family (deliberate fixture regions spanning test
/// files and fixture crates must stay visible).
#[derive(Default)]
struct DuplicateCoverage {
  groups: Vec<GroupCoverage>,
}

/// The source ranges of one AST/sub-AST duplicate group.
#[derive(Default)]
struct GroupCoverage {
  by_file: HashMap<PathBuf, Vec<LineRange>>,
}

impl DuplicateCoverage {
  fn from_matched_groups(ast_groups: &MatchedGroups, precise_sub_groups: Option<&MatchedGroups>) -> Self {
    let mut coverage = Self::default();
    for group in ast_groups.exact.iter().chain(ast_groups.near.iter()) {
      coverage.add_group(group);
    }
    if let Some(sub_groups) = precise_sub_groups {
      for group in sub_groups.exact.iter().chain(sub_groups.near.iter()) {
        coverage.add_group(group);
      }
    }
    coverage
  }

  fn add_group(&mut self, group: &DuplicateGroup) {
    let mut group_coverage = GroupCoverage::default();
    for member in &group.members {
      push_unit_range(&mut group_coverage.by_file, member);
    }
    self.groups.push(group_coverage);
  }

  /// Split groups into uncovered (kept) and covered; covered groups are
  /// tagged `group.covered-by-ast` and their covering group index is
  /// recorded so the survivor can carry an `also seen as` note.
  fn split_substantially_covered(
    &self,
    groups: Vec<DuplicateGroup>,
    policy: &suppression::SuppressionPolicy,
    suppressed: &mut Vec<DuplicateGroup>,
    notes: &mut Vec<CoverageNoteSource>,
  ) -> Vec<DuplicateGroup> {
    if self.groups.is_empty() || !policy.is_enabled(suppression::RuleId::GroupCoveredByAst) {
      return groups;
    }
    let mut kept = Vec::new();
    for mut group in groups {
      if let Some(coverer) = self.covering_group_index(&group) {
        notes.push(CoverageNoteSource {
          coverer,
          dimension: group.dimension,
          match_kind: group.match_kind,
        });
        group.suppressed = policy.allow(suppression::RuleId::GroupCoveredByAst);
        suppressed.push(group);
      } else {
        kept.push(group);
      }
    }
    kept
  }

  fn covering_group_index(&self, group: &DuplicateGroup) -> Option<usize> {
    self
      .groups
      .iter()
      .position(|coverage| group.members.iter().all(|member| coverage.covers_unit(member)))
  }
}

/// One cross-dimension coverage event: the AST/sub group at `coverer` (in
/// [`DuplicateCoverage`] construction order) covered a generic group.
struct CoverageNoteSource {
  coverer:    usize,
  dimension:  DetectionDimension,
  match_kind: grouper::MatchKind,
}

impl GroupCoverage {
  fn covers_unit(&self, unit: &CodeUnit) -> bool {
    if unit.line_start > unit.line_end {
      return false;
    }
    self.covered_ratio(unit) >= PRECISE_COVERAGE_SUPPRESSION_RATIO
  }

  fn covered_ratio(&self, unit: &CodeUnit) -> f64 {
    let Some(ranges) = self.by_file.get(&unit.file) else {
      return 0.0;
    };
    let unit_range = LineRange {
      start: unit.line_start,
      end:   unit.line_end,
    };
    let mut intersections: Vec<LineRange> = ranges.iter().filter_map(|range| range.intersection(unit_range)).collect();
    if intersections.is_empty() {
      return 0.0;
    }

    intersections.sort_by_key(|range| (range.start, range.end));
    let mut covered = 0_usize;
    let mut current = intersections[0];
    for range in intersections.into_iter().skip(1) {
      if range.start <= current.end.saturating_add(1) {
        current.end = current.end.max(range.end);
      } else {
        covered += current.len();
        current = range;
      }
    }
    covered += current.len();

    let span = grouper::unit_line_count(unit);
    if span == 0 { 0.0 } else { covered as f64 / span as f64 }
  }
}

/// Tag generic token/line groups contained within already-kept neighbors.
///
/// Candidates are ranked concept-first: a group whose members span the
/// longest coherent region wins over fragment groups that overlap it, so a
/// compact intentional shape is never replaced by a shifted or stitched
/// fragment that happens to repeat in more places. Contained groups move to
/// the suppressed population tagged `group.overlap-contained`.
fn suppress_overlapping_groups(
  groups: Vec<DuplicateGroup>,
  policy: &suppression::SuppressionPolicy,
  suppressed: &mut Vec<DuplicateGroup>,
) -> Vec<DuplicateGroup> {
  if !policy.is_enabled(suppression::RuleId::GroupOverlapContained) {
    return groups;
  }
  let mut sorted = groups;
  sorted.sort_by(|a, b| {
    group_max_member_span(b)
      .cmp(&group_max_member_span(a))
      .then_with(|| b.members.len().cmp(&a.members.len()))
      .then(grouper::compare_similarity_desc(a.similarity, b.similarity))
      .then_with(|| group_start_key(a).cmp(&group_start_key(b)))
      .then_with(|| a.fingerprint.cmp(&b.fingerprint))
  });

  let mut kept: Vec<DuplicateGroup> = Vec::new();
  for mut group in sorted {
    if is_covered_by_kept(&group, &kept, SAME_DIMENSION_OVERLAP_SUPPRESSION_RATIO) {
      group.suppressed = policy.allow(suppression::RuleId::GroupOverlapContained);
      suppressed.push(group);
    } else {
      kept.push(group);
    }
  }
  kept
}

/// Longest single-member span in a group, in lines.
fn group_max_member_span(group: &DuplicateGroup) -> usize {
  group.members.iter().map(grouper::unit_line_count).max().unwrap_or(0)
}

fn is_covered_by_kept(group: &DuplicateGroup, kept: &[DuplicateGroup], min_overlap_ratio: f64) -> bool {
  kept
    .iter()
    .any(|kept_group| group_is_covered_by_group(group, kept_group, min_overlap_ratio))
}

fn group_is_covered_by_group(group: &DuplicateGroup, kept_group: &DuplicateGroup, min_overlap_ratio: f64) -> bool {
  group.dimension == kept_group.dimension
    && group.members.iter().all(|member| {
      kept_group
        .members
        .iter()
        .any(|kept_member| member.file == kept_member.file && line_overlap_ratio(member, kept_member) >= min_overlap_ratio)
    })
}

fn line_overlap_ratio(candidate: &CodeUnit, representative: &CodeUnit) -> f64 {
  let candidate_range = LineRange {
    start: candidate.line_start,
    end:   candidate.line_end,
  };
  let representative_range = LineRange {
    start: representative.line_start,
    end:   representative.line_end,
  };
  let Some(overlap) = candidate_range.intersection(representative_range) else {
    return 0.0;
  };
  let candidate_span = grouper::unit_line_count(candidate);
  if candidate_span == 0 {
    0.0
  } else {
    overlap.len() as f64 / candidate_span as f64
  }
}

/// Compute exact and near groups for one unit set.
fn compute_matched_groups(units: &[CodeUnit], similarity_threshold: f64, dimension: DetectionDimension) -> MatchedGroups {
  let exact = grouper::group_exact_duplicates_for(units, dimension);
  let exact_fingerprints = grouper::member_fingerprints(&exact);
  let near = grouper::find_near_duplicates_for(units, similarity_threshold, &exact_fingerprints, dimension);

  MatchedGroups {
    exact,
    near,
  }
}

/// Extract fallback sub-units from top-level normalized AST bodies.
fn fallback_sub_units(units: &[CodeUnit], min_sub_nodes: usize) -> Vec<CodeUnit> {
  units
    .iter()
    .flat_map(|unit| {
      extractor::extract_sub_units(&unit.body, min_sub_nodes)
        .into_iter()
        .map(|sub_unit| CodeUnit {
          suppressed:   None,
          parent_chain: None,
          kind:         sub_unit.kind,
          name:         sub_unit.description,
          file:         unit.file.clone(),
          line_start:   unit.line_start,
          line_end:     unit.line_end,
          signature:    node::NormalizedNode::leaf(node::NodeKind::Opaque),
          body:         sub_unit.node.clone(),
          fingerprint:  fingerprint::Fingerprint::from_node(&sub_unit.node),
          node_count:   sub_unit.node_count,
          parent_name:  Some(unit.name.clone()),
          is_test:      unit.is_test,
        })
    })
    .collect()
}

/// Iterate every group of every dimension before ignore filtering.
fn all_unfiltered_groups<'a>(
  ast_groups: &'a MatchedGroups,
  sub_groups: &'a MatchedGroups,
  generic_groups: &'a GenericGroups,
) -> impl Iterator<Item = &'a DuplicateGroup> {
  ast_groups
    .exact
    .iter()
    .chain(ast_groups.near.iter())
    .chain(sub_groups.exact.iter())
    .chain(sub_groups.near.iter())
    .chain(generic_groups.token_normalized.exact.iter())
    .chain(generic_groups.token_normalized.near.iter())
    .chain(generic_groups.token_raw_exact.iter())
    .chain(generic_groups.line_exact.iter())
    .chain(generic_groups.suppressed.iter())
}

/// Collect all group fingerprints before ignore filtering.
fn collect_group_fingerprints(
  ast_groups: &MatchedGroups,
  sub_groups: &MatchedGroups,
  generic_groups: &GenericGroups,
) -> HashSet<Fingerprint> {
  all_unfiltered_groups(ast_groups, sub_groups, generic_groups)
    .map(|group| group.fingerprint)
    .collect()
}

/// Collect each group's member content fingerprints before ignore filtering.
fn collect_member_fingerprint_sets(
  ast_groups: &MatchedGroups,
  sub_groups: &MatchedGroups,
  generic_groups: &GenericGroups,
) -> Vec<HashSet<Fingerprint>> {
  all_unfiltered_groups(ast_groups, sub_groups, generic_groups)
    .map(|group| group.members.iter().map(|member| member.fingerprint).collect())
    .collect()
}

/// Apply ignore filtering to exact and near groups.
fn filter_matched_groups(groups: MatchedGroups, ignore_file: &ignore::IgnoreFile) -> MatchedGroups {
  MatchedGroups {
    exact: ignore::filter_ignored(groups.exact, ignore_file),
    near:  ignore::filter_ignored(groups.near, ignore_file),
  }
}

/// Apply ignore filtering to generic groups.
///
/// The suppressed population is filtered too: the registry is authoritative,
/// so an ignored group never reappears even under `--show-suppressed`.
fn filter_generic_groups(groups: GenericGroups, ignore_file: &ignore::IgnoreFile) -> GenericGroups {
  GenericGroups {
    token_normalized: filter_matched_groups(groups.token_normalized, ignore_file),
    token_raw_exact:  ignore::filter_ignored(groups.token_raw_exact, ignore_file),
    line_exact:       ignore::filter_ignored(groups.line_exact, ignore_file),
    suppressed:       ignore::filter_ignored(groups.suppressed, ignore_file),
  }
}

#[cfg(test)]
mod tests {
  use std::collections::BTreeSet;
  use std::fs;
  use std::path::Path;

  use tempfile::TempDir;

  use super::*;
  use crate::analyzer::LanguageAnalyzer;
  use crate::code_unit::CodeUnitKind;
  use crate::config::AnalysisConfig;
  use crate::grouper::MatchKind;
  use crate::node::BinOpKind;
  use crate::node::LiteralKind;
  use crate::node::NodeKind;
  use crate::node::NormalizedNode;
  use crate::node::PlaceholderKind;

  fn test_body(value: &str) -> NormalizedNode {
    NormalizedNode::with_children(NodeKind::Block, vec![NormalizedNode::leaf(NodeKind::Token(value.to_string()))])
  }

  fn test_unit(kind: CodeUnitKind, name: &str, file: &Path, line_start: usize, line_end: usize, body: NormalizedNode) -> CodeUnit {
    CodeUnit {
      suppressed: None,
      parent_chain: None,
      kind,
      name: name.to_string(),
      file: file.to_path_buf(),
      line_start,
      line_end,
      signature: NormalizedNode::leaf(NodeKind::Opaque),
      fingerprint: Fingerprint::from_node(&body),
      node_count: crate::node::count_nodes(&body),
      body,
      parent_name: None,
      is_test: false,
    }
  }

  fn test_group(dimension: DetectionDimension, fingerprint_seed: &str, members: Vec<CodeUnit>) -> DuplicateGroup {
    crate::output::test_support::duplicate_group(
      dimension,
      MatchKind::Exact,
      Fingerprint::from_bytes(fingerprint_seed.as_bytes()),
      1.0,
      members,
    )
  }

  fn line_only_config(root: &Path, min_lines: usize) -> Config {
    Config {
      root: root.to_path_buf(),
      line_min_lines: min_lines,
      enabled_dimensions: BTreeSet::from([DetectionDimension::Line]),
      ..Config::default()
    }
  }

  fn ast_only_config(root: &Path, similarity_threshold: f64) -> Config {
    Config {
      root: root.to_path_buf(),
      similarity_threshold,
      enabled_dimensions: BTreeSet::from([DetectionDimension::Ast]),
      ..Config::default()
    }
  }

  fn paired_function_units(first: &Path, second: &Path, first_body: NormalizedNode, second_body: NormalizedNode) -> Vec<CodeUnit> {
    vec![
      test_unit(CodeUnitKind::Function, "first", first, 10, 20, first_body),
      test_unit(CodeUnitKind::Function, "second", second, 30, 40, second_body),
    ]
  }

  // jscpd:ignore-start

  fn variable(index: usize) -> NormalizedNode {
    NormalizedNode::leaf(NodeKind::Placeholder(PlaceholderKind::Variable, index))
  }

  fn literal_int() -> NormalizedNode {
    NormalizedNode::leaf(NodeKind::Literal(LiteralKind::Int))
  }

  fn block(children: Vec<NormalizedNode>) -> NormalizedNode {
    NormalizedNode::with_children(NodeKind::Block, children)
  }

  fn binary(op: BinOpKind, left: NormalizedNode, right: NormalizedNode) -> NormalizedNode {
    NormalizedNode::with_children(NodeKind::BinaryOp(op), vec![left, right])
  }

  // jscpd:ignore-end

  fn if_with_then_branch(then_branch: NormalizedNode) -> NormalizedNode {
    NormalizedNode::with_children(NodeKind::If, vec![
      binary(BinOpKind::Gt, variable(0), literal_int()),
      then_branch,
      NormalizedNode::none(),
    ])
  }

  struct TestAnalyzer;

  impl LanguageAnalyzer for TestAnalyzer {
    fn file_extensions(&self) -> &[&str] {
      &["rs"]
    }

    fn parse_file(
      &self,
      path: &Path,
      source: &str,
      _config: &AnalysisConfig,
    ) -> Result<Vec<CodeUnit>, Box<dyn std::error::Error + Send + Sync>> {
      let body = NormalizedNode::leaf(NodeKind::Opaque);
      Ok(vec![CodeUnit {
        suppressed: None,
        parent_chain: None,
        kind: CodeUnitKind::Function,
        name: "test_unit".to_string(),
        file: path.to_path_buf(),
        line_start: 1,
        line_end: source.lines().count().max(1),
        signature: NormalizedNode::leaf(NodeKind::Opaque),
        fingerprint: Fingerprint::from_node(&body),
        node_count: 1,
        body,
        parent_name: None,
        is_test: true,
      }])
    }
  }

  #[test]
  fn exclude_tests_keeps_non_test_generic_text() {
    let tmp = TempDir::new().unwrap();
    let first_test = tmp.path().join("first.rs");
    let second_test = tmp.path().join("second.rs");
    let first_doc = tmp.path().join("first.md");
    let second_doc = tmp.path().join("second.md");
    fs::write(&first_test, "test duplicated\nbody\n").unwrap();
    fs::write(&second_test, "test duplicated\nbody\n").unwrap();
    fs::write(&first_doc, "if docs match {\nlet value = 1;\nreturn value;\n}\n").unwrap();
    fs::write(&second_doc, "if docs match {\nlet value = 1;\nreturn value;\n}\n").unwrap();

    let config = Config {
      root: tmp.path().to_path_buf(),
      exclude_tests: true,
      line_min_lines: 3,
      enabled_dimensions: BTreeSet::from([DetectionDimension::Line]),
      ..Config::default()
    };

    let result = analyze_with_generic(
      &TestAnalyzer,
      &[first_test.clone(), second_test.clone()],
      &[first_test, second_test, first_doc.clone(), second_doc.clone()],
      &config,
    )
    .unwrap();

    assert_eq!(result.line_exact_groups.len(), 1);
    assert!(
      result.line_exact_groups[0]
        .members
        .iter()
        .all(|member| member.file == first_doc || member.file == second_doc)
    );
  }

  // jscpd:ignore-start

  #[test]
  fn generic_groups_covered_by_ast_duplicates_are_suppressed() {
    let tmp = TempDir::new().unwrap();
    let first = tmp.path().join("first.rs");
    let second = tmp.path().join("second.rs");
    let ast_body = test_body("same ast body");
    let line_body = test_body("same line window");
    let units = vec![
      test_unit(CodeUnitKind::Function, "first", &first, 10, 20, ast_body.clone()),
      test_unit(CodeUnitKind::Function, "second", &second, 30, 40, ast_body),
    ];
    let line_units = vec![
      test_unit(CodeUnitKind::LineWindow, "line", &first, 12, 16, line_body.clone()),
      test_unit(CodeUnitKind::LineWindow, "line", &second, 32, 36, line_body),
    ];
    let config = Config {
      root: tmp.path().to_path_buf(),
      enabled_dimensions: BTreeSet::from([DetectionDimension::Ast, DetectionDimension::Line]),
      ..Config::default()
    };

    let result = analyze_units_with_generic(&units, &[], &[], &[], &line_units, Vec::new(), &config).unwrap();

    assert_eq!(result.exact_groups.len(), 1);
    assert!(result.line_exact_groups.is_empty());
  }

  #[test]
  fn generic_groups_mostly_covered_by_ast_duplicates_are_suppressed() {
    let tmp = TempDir::new().unwrap();
    let first = tmp.path().join("first.rs");
    let second = tmp.path().join("second.rs");
    let ast_body = test_body("same ast body");
    let line_body = test_body("same line window");
    let units = vec![
      test_unit(CodeUnitKind::Function, "first", &first, 10, 20, ast_body.clone()),
      test_unit(CodeUnitKind::Function, "second", &second, 30, 40, ast_body),
    ];
    let line_units = vec![
      test_unit(CodeUnitKind::LineWindow, "line", &first, 8, 17, line_body.clone()),
      test_unit(CodeUnitKind::LineWindow, "line", &second, 28, 37, line_body),
    ];
    let config = Config {
      root: tmp.path().to_path_buf(),
      enabled_dimensions: BTreeSet::from([DetectionDimension::Ast, DetectionDimension::Line]),
      ..Config::default()
    };

    let result = analyze_units_with_generic(&units, &[], &[], &[], &line_units, Vec::new(), &config).unwrap();

    assert_eq!(result.exact_groups.len(), 1);
    assert!(result.line_exact_groups.is_empty());
  }

  #[test]
  fn generic_groups_with_only_edge_overlap_are_kept() {
    let tmp = TempDir::new().unwrap();
    let first = tmp.path().join("first.rs");
    let second = tmp.path().join("second.rs");
    let ast_body = test_body("same ast body");
    let line_body = test_body("same line window");
    let units = vec![
      test_unit(CodeUnitKind::Function, "first", &first, 10, 20, ast_body.clone()),
      test_unit(CodeUnitKind::Function, "second", &second, 30, 40, ast_body),
    ];
    let line_units = vec![
      test_unit(CodeUnitKind::LineWindow, "line", &first, 7, 14, line_body.clone()),
      test_unit(CodeUnitKind::LineWindow, "line", &second, 27, 34, line_body),
    ];
    let config = Config {
      root: tmp.path().to_path_buf(),
      enabled_dimensions: BTreeSet::from([DetectionDimension::Ast, DetectionDimension::Line]),
      ..Config::default()
    };

    let result = analyze_units_with_generic(&units, &[], &[], &[], &line_units, Vec::new(), &config).unwrap();

    assert_eq!(result.exact_groups.len(), 1);
    assert_eq!(result.line_exact_groups.len(), 1);
  }

  #[test]
  fn generic_groups_outside_ast_duplicates_are_kept() {
    let tmp = TempDir::new().unwrap();
    let first = tmp.path().join("first.rs");
    let second = tmp.path().join("second.rs");
    let ast_body = test_body("same ast body");
    let line_body = test_body("same line window");
    let units = vec![
      test_unit(CodeUnitKind::Function, "first", &first, 10, 20, ast_body.clone()),
      test_unit(CodeUnitKind::Function, "second", &second, 30, 40, ast_body),
    ];
    let line_units = vec![
      test_unit(CodeUnitKind::LineWindow, "line", &first, 50, 54, line_body.clone()),
      test_unit(CodeUnitKind::LineWindow, "line", &second, 60, 64, line_body),
    ];
    let config = Config {
      root: tmp.path().to_path_buf(),
      enabled_dimensions: BTreeSet::from([DetectionDimension::Ast, DetectionDimension::Line]),
      ..Config::default()
    };

    let result = analyze_units_with_generic(&units, &[], &[], &[], &line_units, Vec::new(), &config).unwrap();

    assert_eq!(result.exact_groups.len(), 1);
    assert_eq!(result.line_exact_groups.len(), 1);
  }

  #[test]
  fn line_only_dimension_does_not_use_ast_coverage() {
    let tmp = TempDir::new().unwrap();
    let first = tmp.path().join("first.rs");
    let second = tmp.path().join("second.rs");
    let ast_body = test_body("same ast body");
    let line_body = test_body("same line window");
    let units = vec![
      test_unit(CodeUnitKind::Function, "first", &first, 10, 20, ast_body.clone()),
      test_unit(CodeUnitKind::Function, "second", &second, 30, 40, ast_body),
    ];
    let line_units = vec![
      test_unit(CodeUnitKind::LineWindow, "line", &first, 12, 16, line_body.clone()),
      test_unit(CodeUnitKind::LineWindow, "line", &second, 32, 36, line_body),
    ];
    let config = Config {
      root: tmp.path().to_path_buf(),
      enabled_dimensions: BTreeSet::from([DetectionDimension::Line]),
      ..Config::default()
    };

    let result = analyze_units_with_generic(&units, &[], &[], &[], &line_units, Vec::new(), &config).unwrap();

    assert!(result.exact_groups.is_empty());
    assert_eq!(result.line_exact_groups.len(), 1);
  }

  // jscpd:ignore-end

  // jscpd:ignore-start

  #[test]
  fn ignore_filtering_keeps_group_fingerprints_live_pre_policy() {
    let tmp = TempDir::new().unwrap();
    let first = tmp.path().join("first.rs");
    let second = tmp.path().join("second.rs");
    let body = test_body("same ast body");
    let units = paired_function_units(&first, &second, body.clone(), body);
    let config = ast_only_config(tmp.path(), Config::default().similarity_threshold);
    let baseline = analyze_units_with_generic(&units, &[], &[], &[], &[], Vec::new(), &config).unwrap();
    assert_eq!(baseline.exact_groups.len(), 1);
    let ignored_fingerprint = baseline.exact_groups[0].fingerprint;
    let mut ignore_file = ignore::IgnoreFile::default();
    ignore::add_ignore(
      &mut ignore_file,
      &ignored_fingerprint,
      Some("intentional duplicate".to_string()),
      Vec::new(),
    );
    ignore::save_ignore_file(tmp.path(), &ignore_file).unwrap();

    let result = analyze_units_with_generic(&units, &[], &[], &[], &[], Vec::new(), &config).unwrap();

    assert!(result.exact_groups.is_empty());
    assert!(
      result.all_fingerprints.contains(&ignored_fingerprint),
      "ignore policy must not erase pre-filter liveness"
    );
  }

  #[test]
  fn member_fingerprint_ignores_keep_member_sets_live_pre_policy() {
    let tmp = TempDir::new().unwrap();
    let first = tmp.path().join("first.rs");
    let second = tmp.path().join("second.rs");
    let units = paired_function_units(&first, &second, test_body("first near body"), test_body("second near body"));
    let config = ast_only_config(tmp.path(), 0.0);
    let baseline = analyze_units_with_generic(&units, &[], &[], &[], &[], Vec::new(), &config).unwrap();
    assert_eq!(baseline.near_groups.len(), 1);
    let near_group = &baseline.near_groups[0];
    let member_fingerprints: Vec<_> = near_group.members.iter().map(|member| member.fingerprint.to_hex()).collect();
    let expected_member_set: std::collections::HashSet<_> = near_group.members.iter().map(|member| member.fingerprint).collect();
    let mut ignore_file = ignore::IgnoreFile::default();
    ignore::add_ignore_with_member_fingerprints(
      &mut ignore_file,
      &Fingerprint::from_bytes(b"old near group fingerprint"),
      Some("membership-stable near duplicate".to_string()),
      Vec::new(),
      member_fingerprints,
    );
    ignore::save_ignore_file(tmp.path(), &ignore_file).unwrap();

    let result = analyze_units_with_generic(&units, &[], &[], &[], &[], Vec::new(), &config).unwrap();

    assert!(result.near_groups.is_empty());
    assert!(result.all_fingerprints.contains(&near_group.fingerprint));
    assert!(
      result.all_member_fingerprint_sets.iter().any(|set| set == &expected_member_set),
      "member-fingerprint ignores must not erase pre-filter liveness"
    );
  }

  // jscpd:ignore-end

  #[test]
  fn same_dimension_overlap_requires_one_kept_group_to_cover_all_members() {
    let tmp = TempDir::new().unwrap();
    let first = tmp.path().join("first.rs");
    let second = tmp.path().join("second.rs");
    let third = tmp.path().join("third.rs");
    let fourth = tmp.path().join("fourth.rs");
    let body = test_body("same line window");
    let candidate = test_group(DetectionDimension::Line, "candidate", vec![
      test_unit(CodeUnitKind::LineWindow, "candidate", &first, 11, 15, body.clone()),
      test_unit(CodeUnitKind::LineWindow, "candidate", &third, 31, 35, body.clone()),
    ]);
    let first_kept = test_group(DetectionDimension::Line, "first_kept", vec![
      test_unit(CodeUnitKind::LineWindow, "kept", &first, 10, 14, body.clone()),
      test_unit(CodeUnitKind::LineWindow, "kept", &second, 20, 24, body.clone()),
    ]);
    let second_kept = test_group(DetectionDimension::Line, "second_kept", vec![
      test_unit(CodeUnitKind::LineWindow, "kept", &third, 30, 34, body.clone()),
      test_unit(CodeUnitKind::LineWindow, "kept", &fourth, 40, 44, body),
    ]);

    assert!(!is_covered_by_kept(
      &candidate,
      &[first_kept, second_kept],
      SAME_DIMENSION_OVERLAP_SUPPRESSION_RATIO
    ));
  }

  // jscpd:ignore-start

  #[test]
  fn shifted_line_window_groups_merge_into_concept_level_group() {
    let tmp = TempDir::new().unwrap();
    let first = tmp.path().join("first.rs");
    let second = tmp.path().join("second.rs");
    // One eight-line duplicated concept; sliding five-line windows would
    // otherwise report it as four shifted fragment groups.
    let concept = "\
let total = alpha + beta;
let scaled = total * gamma;
let checked = scaled - delta;
let rounded = checked + epsilon;
let bounded = rounded * zeta;
let shifted = bounded - eta;
let summed = shifted + theta;
return summed * iota;
";
    fs::write(&first, concept).unwrap();
    fs::write(&second, concept).unwrap();

    let config = line_only_config(tmp.path(), 5);

    let result = analyze_with_generic(&TestAnalyzer, &[], &[first, second], &config).unwrap();

    assert_eq!(result.line_exact_groups.len(), 1);
    let group = &result.line_exact_groups[0];
    assert_eq!(group.members.len(), 2);
    for member in &group.members {
      assert_eq!((member.line_start, member.line_end), (1, 8));
    }
  }

  // jscpd:ignore-end

  fn setter_body() -> NormalizedNode {
    use crate::node::PlaceholderKind;
    let var = |index| NormalizedNode::leaf(NodeKind::Placeholder(PlaceholderKind::Variable, index));
    let field = NormalizedNode::with_children(NodeKind::FieldAccess, vec![var(0), var(1)]);
    let assign = NormalizedNode::with_children(NodeKind::Assign, vec![field, var(2)]);
    NormalizedNode::with_children(NodeKind::Block, vec![assign, var(0)])
  }

  fn unit_with_body(name: &str, kind: CodeUnitKind, body: NormalizedNode) -> CodeUnit {
    let mut unit = crate::output::test_support::make_unit(name, "src/sample.rs", 1, 4);
    unit.kind = kind;
    unit.fingerprint = Fingerprint::from_node(&body);
    unit.body = body;
    unit
  }

  #[test]
  fn trivial_setter_pairs_group_as_suppressed_with_their_rule() {
    let units = vec![
      unit_with_body("Gauge::with_a", CodeUnitKind::Method, setter_body()),
      unit_with_body("Gauge::with_b", CodeUnitKind::Method, setter_body()),
    ];
    let result = analyze_units(&units, Vec::new(), &Config::default()).unwrap();

    assert!(result.exact_groups.is_empty());
    assert_eq!(result.suppressed_groups.len(), 1);
    let group = &result.suppressed_groups[0];
    assert_eq!(group.suppressed, Some(crate::suppression::RuleId::AstSetterReturningSelf));
    assert!(result.all_fingerprints.contains(&group.fingerprint));
  }

  #[test]
  fn mixed_groups_stay_visible_with_tagged_members() {
    // An impl-block unit shares the setter's content but is not a
    // taggable kind, so the group stays visible and only the method
    // member carries the rule.
    let units = vec![
      unit_with_body("Gauge::with_a", CodeUnitKind::Method, setter_body()),
      unit_with_body("twin impl", CodeUnitKind::ImplBlock, setter_body()),
    ];
    let result = analyze_units(&units, Vec::new(), &Config::default()).unwrap();

    assert_eq!(result.exact_groups.len(), 1);
    let group = &result.exact_groups[0];
    assert!(group.suppressed.is_none());
    let tagged: Vec<_> = group.members.iter().filter(|member| member.suppressed.is_some()).collect();
    assert_eq!(tagged.len(), 1);
    assert_eq!(tagged[0].name, "Gauge::with_a");
  }

  #[test]
  fn ignore_entries_hide_suppressed_groups_and_count_as_ignored() {
    // The registry is authoritative even over rule-suppressed findings:
    // an ignored suppressed group leaves `suppressed_groups`, increments
    // the ignored count, and stays live for cleanup accounting.
    let tmp = TempDir::new().unwrap();
    let units = vec![
      unit_with_body("Gauge::with_a", CodeUnitKind::Method, setter_body()),
      unit_with_body("Gauge::with_b", CodeUnitKind::Method, setter_body()),
    ];
    let config = Config {
      root: tmp.path().to_path_buf(),
      ..Config::default()
    };
    let unfiltered = analyze_units(&units, Vec::new(), &config).unwrap();
    let group_fp = unfiltered.suppressed_groups[0].fingerprint;

    let mut ignore_file = ignore::IgnoreFile::default();
    ignore::add_ignore(&mut ignore_file, &group_fp, None, vec![]);
    ignore::save_ignore_file(tmp.path(), &ignore_file).unwrap();

    let result = analyze_units(&units, Vec::new(), &config).unwrap();
    assert!(result.suppressed_groups.is_empty());
    assert_eq!(result.stats.ignored_group_count, 1);
    assert!(result.all_fingerprints.contains(&group_fp));
  }

  #[test]
  fn covering_groups_carry_also_seen_notes() {
    // A line window fully covered by one AST group is tagged
    // group.covered-by-ast and the covering group records the shadow.
    let tmp = TempDir::new().unwrap();
    let first = tmp.path().join("first.rs");
    let second = tmp.path().join("second.rs");
    let body = "\
let weight = mass * pull;
let drag = weight / spread;
let lift = drag - offset;
let glide = lift * trim;
let sink = glide + ballast;
";
    fs::write(&first, body).unwrap();
    fs::write(&second, body).unwrap();

    let mut units = vec![
      crate::output::test_support::make_unit("alpha_calc", "first.rs", 1, 5),
      crate::output::test_support::make_unit("beta_calc", "second.rs", 1, 5),
    ];
    for (unit, file) in units.iter_mut().zip([&first, &second]) {
      unit.file = file.clone();
    }

    let config = Config {
      root: tmp.path().to_path_buf(),
      enabled_dimensions: BTreeSet::from([DetectionDimension::Ast, DetectionDimension::Line]),
      ..Config::default()
    };
    let mut all_line_units = text_units::extract(&first, body, &config).lines;
    all_line_units.append(&mut text_units::extract(&second, body, &config).lines);

    let result = analyze_units_with_generic(&units, &[], &[], &[], &all_line_units, Vec::new(), &config).unwrap();

    assert_eq!(result.exact_groups.len(), 1, "the AST pair stays visible");
    assert!(result.line_exact_groups.is_empty(), "the covered line group is suppressed");
    let covered = result
      .suppressed_groups
      .iter()
      .filter(|group| group.suppressed == Some(crate::suppression::RuleId::GroupCoveredByAst))
      .count();
    assert_eq!(covered, 1);
    let notes = &result.exact_groups[0].also_seen;
    assert_eq!(notes.len(), 1);
    assert_eq!(notes[0].dimension, DetectionDimension::Line);
    assert_eq!(notes[0].group_count, 1);
  }

  #[test]
  fn fully_suppressed_window_groups_move_to_suppressed_groups() {
    // A duplicated chain-tail fragment groups, but every member carries
    // the chain-tail tag, so the group lands in `suppressed_groups` with
    // the rule while staying out of the visible line groups. Its
    // fingerprint stays live for ignore-entry accounting.
    let tmp = TempDir::new().unwrap();
    let first = tmp.path().join("first.rs");
    let second = tmp.path().join("second.rs");
    let tail = "\
builder()
    .alpha(one)
    .bravo(two)
    .charlie(three)
    .delta(four);
";
    fs::write(&first, tail).unwrap();
    fs::write(&second, tail).unwrap();

    let config = line_only_config(tmp.path(), 5);
    let result = analyze_with_generic(&TestAnalyzer, &[], &[first, second], &config).unwrap();

    assert!(result.line_exact_groups.is_empty());
    assert_eq!(result.suppressed_groups.len(), 1);
    let group = &result.suppressed_groups[0];
    assert_eq!(group.suppressed, Some(crate::suppression::RuleId::LineChainTail));
    assert_eq!(group.members.len(), 2);
    assert!(result.all_fingerprints.contains(&group.fingerprint));
  }

  #[test]
  fn merged_line_window_groups_keep_location_independent_fingerprints() {
    let concept = "\
let total = alpha + beta;
let scaled = total * gamma;
let checked = scaled - delta;
let rounded = checked + epsilon;
let bounded = rounded * zeta;
let shifted = bounded - eta;
";
    let run = |prefix: &str| {
      let tmp = TempDir::new().unwrap();
      let first = tmp.path().join("first.rs");
      let second = tmp.path().join("second.rs");
      fs::write(&first, format!("{prefix}{concept}")).unwrap();
      fs::write(&second, concept).unwrap();
      let config = line_only_config(tmp.path(), 5);
      analyze_with_generic(&TestAnalyzer, &[], &[first, second], &config).unwrap()
    };

    let plain = run("");
    let shifted = run("alpha computes unrelated prefix totals here\n\n");
    assert_eq!(plain.line_exact_groups.len(), 1);
    assert_eq!(shifted.line_exact_groups.len(), 1);
    assert_eq!(plain.line_exact_groups[0].fingerprint, shifted.line_exact_groups[0].fingerprint,);
  }

  #[test]
  fn concept_level_line_group_wins_over_fragment_with_more_members() {
    let body = test_body("same line window");
    let make_unit = |file: &Path, start: usize, end: usize| CodeUnit {
      suppressed:   None,
      parent_chain: None,
      kind:         CodeUnitKind::LineWindow,
      name:         "line window".to_string(),
      file:         file.to_path_buf(),
      line_start:   start,
      line_end:     end,
      signature:    NormalizedNode::leaf(NodeKind::Opaque),
      fingerprint:  Fingerprint::from_node(&body),
      node_count:   1,
      body:         body.clone(),
      parent_name:  None,
      is_test:      false,
    };
    let first = PathBuf::from("first.rs");
    let second = PathBuf::from("second.rs");
    let third = PathBuf::from("third.rs");
    let concept = test_group(DetectionDimension::Line, "concept", vec![
      make_unit(&first, 10, 21),
      make_unit(&second, 30, 41),
    ]);
    let fragment = test_group(DetectionDimension::Line, "fragment", vec![
      make_unit(&first, 12, 16),
      make_unit(&second, 32, 36),
      make_unit(&third, 50, 54),
    ]);

    let mut suppressed = Vec::new();
    let kept = suppress_overlapping_groups(
      vec![fragment, concept],
      &crate::suppression::SuppressionPolicy::default(),
      &mut suppressed,
    );

    assert_eq!(kept.len(), 2, "fragment with a member outside survives");
    assert!(suppressed.is_empty());
    assert_eq!(
      kept[0].members[0].line_end - kept[0].members[0].line_start,
      11,
      "the concept-level group is ranked first"
    );

    let covered_fragment = test_group(DetectionDimension::Line, "covered", vec![
      make_unit(&first, 12, 16),
      make_unit(&second, 32, 36),
    ]);
    let concept = test_group(DetectionDimension::Line, "concept", vec![
      make_unit(&first, 10, 21),
      make_unit(&second, 30, 41),
    ]);
    let mut suppressed = Vec::new();
    let kept = suppress_overlapping_groups(
      vec![covered_fragment, concept],
      &crate::suppression::SuppressionPolicy::default(),
      &mut suppressed,
    );
    assert_eq!(kept.len(), 1, "fully covered fragment is tagged away");
    assert_eq!(kept[0].members[0].line_start, 10);
    assert_eq!(suppressed.len(), 1);
    assert_eq!(suppressed[0].suppressed, Some(crate::suppression::RuleId::GroupOverlapContained));
  }

  // jscpd:ignore-start

  #[test]
  fn fixture_loop_bodies_remain_detected_in_line_dimension() {
    // The intentional fixture loop bodies in the exact_dupes and
    // near_dupes test crates must stay discoverable by line detection.
    let tmp = TempDir::new().unwrap();
    let first = tmp.path().join("exact_fixture.rs");
    let second = tmp.path().join("near_fixture.rs");
    fs::write(
      &first,
      "\
pub fn process_data(input: Vec<i32>) -> i32 {
    let mut sum = 0;
    for item in input.iter() {
        if *item > 0 {
            sum += *item;
        }
    }
    sum
}
",
    )
    .unwrap();
    fs::write(
      &second,
      "\
pub fn process_positive(input: Vec<i32>) -> i32 {
    let mut sum = 0;
    for item in input.iter() {
        if *item > 0 {
            sum += *item;
        }
    }
    sum
}
",
    )
    .unwrap();

    let config = line_only_config(tmp.path(), 5);

    let result = analyze_with_generic(&TestAnalyzer, &[], &[first, second], &config).unwrap();

    assert_eq!(result.line_exact_groups.len(), 1);
    let group = &result.line_exact_groups[0];
    for member in &group.members {
      assert!(member.line_start >= 2, "signature lines differ by name");
      assert!(member.line_end <= 9);
      assert!(grouper::unit_line_count(member) >= 5);
    }
  }

  // jscpd:ignore-end

  // jscpd:ignore-start

  #[test]
  fn fallback_sub_ast_trivial_branches_are_not_reported() {
    let tmp = TempDir::new().unwrap();
    let first = tmp.path().join("first.rs");
    let second = tmp.path().join("second.rs");
    let body = if_with_then_branch(block(vec![variable(0)]));
    let units = vec![
      test_unit(CodeUnitKind::Function, "first", &first, 1, 3, body.clone()),
      test_unit(CodeUnitKind::Function, "second", &second, 5, 7, body),
    ];
    let config = Config {
      root: tmp.path().to_path_buf(),
      sub_function: true,
      min_sub_nodes: 1,
      enabled_dimensions: BTreeSet::from([DetectionDimension::SubAst]),
      ..Config::default()
    };

    let result = analyze_units_with_generic(&units, &[], &[], &[], &[], Vec::new(), &config).unwrap();

    assert!(result.sub_exact_groups.is_empty());
  }

  #[test]
  fn fallback_sub_ast_meaningful_branches_are_reported() {
    let tmp = TempDir::new().unwrap();
    let first = tmp.path().join("first.rs");
    let second = tmp.path().join("second.rs");
    let branch = block(vec![binary(BinOpKind::Add, variable(0), literal_int())]);
    let body = if_with_then_branch(branch);
    let units = vec![
      test_unit(CodeUnitKind::Function, "first", &first, 1, 3, body.clone()),
      test_unit(CodeUnitKind::Function, "second", &second, 5, 7, body),
    ];
    let config = Config {
      root: tmp.path().to_path_buf(),
      sub_function: true,
      min_sub_nodes: 1,
      enabled_dimensions: BTreeSet::from([DetectionDimension::SubAst]),
      ..Config::default()
    };

    let result = analyze_units_with_generic(&units, &[], &[], &[], &[], Vec::new(), &config).unwrap();

    assert_eq!(result.sub_exact_groups.len(), 1);
  }

  #[test]
  fn fallback_sub_ast_parent_spans_do_not_suppress_generic_groups() {
    let tmp = TempDir::new().unwrap();
    let first = tmp.path().join("first.rs");
    let second = tmp.path().join("second.rs");
    let branch = block(vec![binary(BinOpKind::Add, variable(0), literal_int())]);
    let body = if_with_then_branch(branch);
    let line_body = test_body("same line window");
    let units = vec![
      test_unit(CodeUnitKind::Function, "first", &first, 1, 20, body.clone()),
      test_unit(CodeUnitKind::Function, "second", &second, 30, 49, body),
    ];
    let line_units = vec![
      test_unit(CodeUnitKind::LineWindow, "line", &first, 10, 14, line_body.clone()),
      test_unit(CodeUnitKind::LineWindow, "line", &second, 40, 44, line_body),
    ];
    let config = Config {
      root: tmp.path().to_path_buf(),
      sub_function: true,
      min_sub_nodes: 1,
      enabled_dimensions: BTreeSet::from([DetectionDimension::SubAst, DetectionDimension::Line]),
      ..Config::default()
    };

    let result = analyze_units_with_generic(&units, &[], &[], &[], &line_units, Vec::new(), &config).unwrap();

    assert_eq!(result.sub_exact_groups.len(), 1);
    assert_eq!(result.line_exact_groups.len(), 1);
  }
}

// jscpd:ignore-end
