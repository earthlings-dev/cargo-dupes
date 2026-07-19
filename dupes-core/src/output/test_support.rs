use std::collections::HashSet;
use std::path::PathBuf;

use crate::AnalysisResult;
use crate::code_unit::CodeUnit;
use crate::code_unit::CodeUnitKind;
use crate::code_unit::DetectionDimension;
use crate::fingerprint::Fingerprint;
use crate::grouper::DuplicateGroup;
use crate::grouper::DuplicationStats;
use crate::grouper::MatchKind;
use crate::node::NodeKind;
use crate::node::NormalizedNode;

fn opaque_fingerprint() -> Fingerprint {
  Fingerprint::from_node(&NormalizedNode::leaf(NodeKind::Opaque))
}

pub(super) fn block_fingerprint() -> Fingerprint {
  Fingerprint::from_node(&NormalizedNode::with_children(NodeKind::Block, vec![]))
}

pub fn make_unit(name: &str, file: &str, line_start: usize, line_end: usize) -> CodeUnit {
  CodeUnit {
    suppressed: None,
    parent_chain: None,
    kind: CodeUnitKind::Function,
    name: name.to_string(),
    file: PathBuf::from(file),
    line_start,
    line_end,
    signature: NormalizedNode::leaf(NodeKind::Opaque),
    body: NormalizedNode::with_children(NodeKind::Block, vec![]),
    fingerprint: opaque_fingerprint(),
    node_count: 10,
    parent_name: None,
    is_test: false,
  }
}

pub fn duplicate_group(
  dimension: DetectionDimension,
  match_kind: MatchKind,
  fingerprint: Fingerprint,
  similarity: f64,
  members: Vec<CodeUnit>,
) -> DuplicateGroup {
  DuplicateGroup {
    suppressed: None,
    also_seen: Vec::new(),
    dimension,
    match_kind,
    fingerprint,
    members,
    similarity,
  }
}

pub(super) fn exact_group(members: Vec<CodeUnit>) -> DuplicateGroup {
  duplicate_group(DetectionDimension::Ast, MatchKind::Exact, opaque_fingerprint(), 1.0, members)
}

pub(super) fn near_group(fingerprint: Fingerprint, similarity: f64, members: Vec<CodeUnit>) -> DuplicateGroup {
  duplicate_group(DetectionDimension::Ast, MatchKind::Near, fingerprint, similarity, members)
}

pub(super) fn stats(
  total_code_units: usize,
  total_lines: usize,
  exact_duplicate_groups: usize,
  exact_duplicate_units: usize,
  near_duplicate_groups: usize,
  near_duplicate_units: usize,
) -> DuplicationStats {
  DuplicationStats {
    total_code_units,
    total_lines,
    exact_duplicate_groups,
    exact_duplicate_units,
    near_duplicate_groups,
    near_duplicate_units,
    ..Default::default()
  }
}

pub(super) fn with_duplicate_lines(
  mut stats: DuplicationStats,
  exact_duplicate_lines: usize,
  near_duplicate_lines: usize,
) -> DuplicationStats {
  stats.exact_duplicate_lines = exact_duplicate_lines;
  stats.near_duplicate_lines = near_duplicate_lines;
  stats
}

pub fn analysis_result(
  stats: DuplicationStats,
  exact_groups: Vec<DuplicateGroup>,
  near_groups: Vec<DuplicateGroup>,
  warnings: Vec<String>,
) -> AnalysisResult {
  AnalysisResult {
    suppressed_groups: Vec::new(),
    stats,
    exact_groups,
    near_groups,
    sub_exact_groups: Vec::new(),
    sub_near_groups: Vec::new(),
    token_normalized_exact_groups: Vec::new(),
    token_normalized_near_groups: Vec::new(),
    token_raw_exact_groups: Vec::new(),
    line_exact_groups: Vec::new(),
    warnings,
    all_fingerprints: HashSet::new(),
    all_member_fingerprint_sets: Vec::new(),
  }
}
