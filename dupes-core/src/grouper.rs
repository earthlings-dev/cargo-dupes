use std::collections::HashMap;

use crate::code_unit::{CodeUnit, CodeUnitKind, DetectionDimension};
use crate::fingerprint::Fingerprint;
use crate::similarity;

/// Whether a group was found by exact equality or similarity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MatchKind {
    /// Every compared signature is exactly equal.
    Exact,
    /// Similarity score is at or above the configured threshold.
    Near,
}

impl std::fmt::Display for MatchKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Exact => write!(f, "exact"),
            Self::Near => write!(f, "near"),
        }
    }
}

/// A group of duplicate code units.
#[derive(Debug, Clone)]
pub struct DuplicateGroup {
    /// Detection dimension that produced this group.
    pub dimension: DetectionDimension,
    /// Exact or near-duplicate match.
    pub match_kind: MatchKind,
    /// Stable group fingerprint derived from the dimension, the match kind,
    /// and a content fingerprint: the shared member fingerprint for exact
    /// groups, a composite of sorted member fingerprints for near groups.
    pub fingerprint: Fingerprint,
    /// The code units in this group.
    pub members: Vec<CodeUnit>,
    /// Similarity score (1.0 for exact duplicates).
    pub similarity: f64,
    /// Suppression rule that hid this group from the default report.
    pub suppressed: Option<crate::suppression::RuleId>,
    /// Redundant shadows of this group in other dimensions, aggregated per
    /// (dimension, match kind).
    pub also_seen: Vec<CoverageNote>,
}

/// A note that a group's duplication also surfaced as redundant groups in
/// another dimension before cross-dimension dedup suppressed them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoverageNote {
    pub dimension: DetectionDimension,
    pub match_kind: MatchKind,
    pub group_count: usize,
}

/// Statistics about duplication in the analyzed codebase.
#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct DuplicationStats {
    pub total_code_units: usize,
    pub total_lines: usize,
    pub exact_duplicate_groups: usize,
    pub exact_duplicate_units: usize,
    pub near_duplicate_groups: usize,
    pub near_duplicate_units: usize,
    pub exact_duplicate_lines: usize,
    pub near_duplicate_lines: usize,
    // Sub-function stats
    pub sub_exact_groups: usize,
    pub sub_exact_units: usize,
    pub sub_near_groups: usize,
    pub sub_near_units: usize,
    // Generic token / line stats
    pub token_normalized_exact_groups: usize,
    pub token_normalized_exact_units: usize,
    pub token_normalized_near_groups: usize,
    pub token_normalized_near_units: usize,
    pub token_raw_exact_groups: usize,
    pub token_raw_exact_units: usize,
    pub line_exact_groups: usize,
    pub line_exact_units: usize,
    // Suppression and ignore accounting
    pub suppressed_unit_count: usize,
    pub suppressed_group_count: usize,
    pub suppressed_by_rule: std::collections::BTreeMap<String, usize>,
    pub ignored_group_count: usize,
}

impl DuplicationStats {
    fn percent_of_total(&self, lines: usize) -> f64 {
        if self.total_lines == 0 {
            0.0
        } else {
            lines as f64 / self.total_lines as f64 * 100.0
        }
    }

    /// Percentage of total lines that are exact duplicates.
    #[must_use]
    pub fn exact_duplicate_percent(&self) -> f64 {
        self.percent_of_total(self.exact_duplicate_lines)
    }

    /// Percentage of total lines that are near duplicates.
    #[must_use]
    pub fn near_duplicate_percent(&self) -> f64 {
        self.percent_of_total(self.near_duplicate_lines)
    }
}

/// Group code units by exact fingerprint match.
#[must_use]
pub fn group_exact_duplicates(units: &[CodeUnit]) -> Vec<DuplicateGroup> {
    group_exact_duplicates_for(units, DetectionDimension::Ast)
}

/// Group code units by exact fingerprint match for a specific dimension.
#[must_use]
pub fn group_exact_duplicates_for(
    units: &[CodeUnit],
    dimension: DetectionDimension,
) -> Vec<DuplicateGroup> {
    let mut groups: HashMap<Fingerprint, Vec<CodeUnit>> = HashMap::new();

    for unit in units {
        groups
            .entry(unit.fingerprint)
            .or_default()
            .push(unit.clone());
    }

    let mut result: Vec<DuplicateGroup> = groups
        .into_iter()
        .filter(|(_, members)| members.len() > 1)
        .map(|(fp, members)| DuplicateGroup {
            suppressed: None,
            also_seen: Vec::new(),
            dimension,
            match_kind: MatchKind::Exact,
            fingerprint: group_fingerprint(dimension, MatchKind::Exact, fp),
            members,
            similarity: 1.0,
        })
        .collect();

    // Sort by group size (largest first), then by fingerprint for stability
    result.sort_by(compare_group_size_desc_then_fingerprint);

    result
}

/// Order groups by member count (largest first), then fingerprint for stability.
pub(crate) fn compare_group_size_desc_then_fingerprint(
    a: &DuplicateGroup,
    b: &DuplicateGroup,
) -> std::cmp::Ordering {
    b.members
        .len()
        .cmp(&a.members.len())
        .then_with(|| a.fingerprint.cmp(&b.fingerprint))
}

/// Order similarity scores descending, treating incomparable values as equal.
pub(crate) fn compare_similarity_desc(a: f64, b: f64) -> std::cmp::Ordering {
    b.partial_cmp(&a).unwrap_or(std::cmp::Ordering::Equal)
}

/// Find near-duplicate groups above the similarity threshold.
/// Pre-filters by `CodeUnitKind` and approximate size to reduce pairwise comparisons.
#[must_use]
pub fn find_near_duplicates(
    units: &[CodeUnit],
    threshold: f64,
    exact_fingerprints: &[Fingerprint],
) -> Vec<DuplicateGroup> {
    find_near_duplicates_for(
        units,
        threshold,
        exact_fingerprints,
        DetectionDimension::Ast,
    )
}

/// Find near-duplicate groups above the similarity threshold for a dimension.
#[must_use]
pub fn find_near_duplicates_for(
    units: &[CodeUnit],
    threshold: f64,
    exact_fingerprints: &[Fingerprint],
    dimension: DetectionDimension,
) -> Vec<DuplicateGroup> {
    // Build set of fingerprints that are already exact duplicates
    let exact_set: std::collections::HashSet<Fingerprint> =
        exact_fingerprints.iter().copied().collect();

    // Filter out units that are already in exact duplicate groups
    let candidates: Vec<&CodeUnit> = units
        .iter()
        .filter(|u| !exact_set.contains(&u.fingerprint))
        .collect();

    if candidates.len() < 2 {
        return Vec::new();
    }

    // Bucket by kind and approximate size range
    let mut buckets: HashMap<(CodeUnitKind, usize), Vec<&CodeUnit>> = HashMap::new();
    for unit in &candidates {
        // Size bucket: group units within 2x of each other
        let size_bucket = if unit.node_count == 0 {
            0
        } else {
            (unit.node_count as f64).log2().floor() as usize
        };
        buckets
            .entry((unit.kind.clone(), size_bucket))
            .or_default()
            .push(unit);
    }

    // Pairwise comparison within buckets
    let mut pairs: Vec<(usize, usize, f64)> = Vec::new();
    let unit_indices: HashMap<*const CodeUnit, usize> = candidates
        .iter()
        .enumerate()
        .map(|(i, u)| (std::ptr::from_ref::<CodeUnit>(*u), i))
        .collect();

    for bucket in buckets.values() {
        if bucket.len() < 2 {
            continue;
        }
        for i in 0..bucket.len() {
            for j in (i + 1)..bucket.len() {
                let score = similarity::similarity_score(&bucket[i].body, &bucket[j].body);
                if score >= threshold {
                    let idx_i = unit_indices[&std::ptr::from_ref::<CodeUnit>(bucket[i])];
                    let idx_j = unit_indices[&std::ptr::from_ref::<CodeUnit>(bucket[j])];
                    pairs.push((idx_i, idx_j, score));
                }
            }
        }
    }

    // Build groups via transitive closure using union-find
    let mut parent: Vec<usize> = (0..candidates.len()).collect();
    let mut scores: HashMap<(usize, usize), f64> = HashMap::new();

    for &(i, j, score) in &pairs {
        union(&mut parent, i, j);
        let key = (i.min(j), i.max(j));
        scores.insert(key, score);
    }

    // Collect groups
    let mut group_map: HashMap<usize, Vec<usize>> = HashMap::new();
    for i in 0..candidates.len() {
        let root = find(&mut parent, i);
        group_map.entry(root).or_default().push(i);
    }

    let mut result: Vec<DuplicateGroup> = group_map
        .into_values()
        .filter(|members| members.len() > 1)
        .map(|member_indices| {
            // Compute minimum similarity within the group
            let mut min_score = f64::INFINITY;
            for &i in &member_indices {
                for &j in &member_indices {
                    if i < j
                        && let Some(&s) = scores.get(&(i, j))
                        && s < min_score
                    {
                        min_score = s;
                    }
                }
            }

            let members: Vec<CodeUnit> = member_indices
                .iter()
                .map(|&i| candidates[i].clone())
                .collect();

            let member_fps: Vec<Fingerprint> = members.iter().map(|m| m.fingerprint).collect();
            let composite_fp = Fingerprint::from_fingerprints(&member_fps);

            DuplicateGroup {
                suppressed: None,
                also_seen: Vec::new(),
                dimension,
                match_kind: MatchKind::Near,
                fingerprint: group_fingerprint(dimension, MatchKind::Near, composite_fp),
                members,
                similarity: if min_score.is_infinite() {
                    threshold
                } else {
                    min_score
                },
            }
        })
        .collect();

    result.sort_by(|a, b| {
        b.members
            .len()
            .cmp(&a.members.len())
            .then(compare_similarity_desc(a.similarity, b.similarity))
            .then_with(|| a.fingerprint.cmp(&b.fingerprint))
    });

    result
}

/// Build a stable group fingerprint tied to dimension, match kind, and content.
fn group_fingerprint(
    dimension: DetectionDimension,
    match_kind: MatchKind,
    content_fingerprint: Fingerprint,
) -> Fingerprint {
    Fingerprint::from_bytes(format!("{dimension}:{match_kind}:{content_fingerprint}").as_bytes())
}

/// Build the stable group fingerprint for an exact group in a dimension.
///
/// Used when the analysis pipeline rebuilds a group from merged window
/// content so the result is identical to a directly grouped window.
pub(crate) fn exact_group_fingerprint(
    dimension: DetectionDimension,
    content_fingerprint: Fingerprint,
) -> Fingerprint {
    group_fingerprint(dimension, MatchKind::Exact, content_fingerprint)
}

/// Return the member fingerprints of every group, in group order.
#[must_use]
pub fn member_fingerprints(groups: &[DuplicateGroup]) -> Vec<Fingerprint> {
    member_fingerprints_iter(groups.iter()).collect()
}

/// Project the member fingerprints of `groups`, in group order.
pub(crate) fn member_fingerprints_iter<'a>(
    groups: impl Iterator<Item = &'a DuplicateGroup> + 'a,
) -> impl Iterator<Item = Fingerprint> + 'a {
    groups.flat_map(|group| group.members.iter().map(|member| member.fingerprint))
}

/// Compute the total number of source lines in a duplicate group.
fn group_line_count(group: &DuplicateGroup) -> usize {
    group.members.iter().map(unit_line_count).sum()
}

/// Compute the number of source lines covered by a code unit.
pub(crate) const fn unit_line_count(unit: &CodeUnit) -> usize {
    unit.line_end.saturating_sub(unit.line_start) + 1
}

/// Compute duplication statistics.
pub fn compute_stats(
    units: &[CodeUnit],
    exact_groups: &[DuplicateGroup],
    near_groups: &[DuplicateGroup],
) -> DuplicationStats {
    let total_lines: usize = units.iter().map(unit_line_count).sum();

    DuplicationStats {
        suppressed_unit_count: 0,
        suppressed_group_count: 0,
        suppressed_by_rule: std::collections::BTreeMap::new(),
        ignored_group_count: 0,
        total_code_units: units.len(),
        total_lines,
        exact_duplicate_groups: exact_groups.len(),
        exact_duplicate_units: exact_groups.iter().map(|g| g.members.len()).sum(),
        near_duplicate_groups: near_groups.len(),
        near_duplicate_units: near_groups.iter().map(|g| g.members.len()).sum(),
        exact_duplicate_lines: exact_groups.iter().map(group_line_count).sum(),
        near_duplicate_lines: near_groups.iter().map(group_line_count).sum(),
        sub_exact_groups: 0,
        sub_exact_units: 0,
        sub_near_groups: 0,
        sub_near_units: 0,
        token_normalized_exact_groups: 0,
        token_normalized_exact_units: 0,
        token_normalized_near_groups: 0,
        token_normalized_near_units: 0,
        token_raw_exact_groups: 0,
        token_raw_exact_units: 0,
        line_exact_groups: 0,
        line_exact_units: 0,
    }
}

/// Compute duplication statistics including sub-function results.
#[must_use]
pub fn compute_stats_with_sub(
    units: &[CodeUnit],
    exact_groups: &[DuplicateGroup],
    near_groups: &[DuplicateGroup],
    sub_exact_groups: &[DuplicateGroup],
    sub_near_groups: &[DuplicateGroup],
) -> DuplicationStats {
    let mut stats = compute_stats(units, exact_groups, near_groups);
    stats.sub_exact_groups = sub_exact_groups.len();
    stats.sub_exact_units = sub_exact_groups.iter().map(|g| g.members.len()).sum();
    stats.sub_near_groups = sub_near_groups.len();
    stats.sub_near_units = sub_near_groups.iter().map(|g| g.members.len()).sum();
    stats
}

/// Add generic token and line group counts to existing stats.
#[must_use]
pub fn with_generic_stats(
    mut stats: DuplicationStats,
    token_normalized_exact_groups: &[DuplicateGroup],
    token_normalized_near_groups: &[DuplicateGroup],
    token_raw_exact_groups: &[DuplicateGroup],
    line_exact_groups: &[DuplicateGroup],
) -> DuplicationStats {
    stats.token_normalized_exact_groups = token_normalized_exact_groups.len();
    stats.token_normalized_exact_units = token_normalized_exact_groups
        .iter()
        .map(|g| g.members.len())
        .sum();
    stats.token_normalized_near_groups = token_normalized_near_groups.len();
    stats.token_normalized_near_units = token_normalized_near_groups
        .iter()
        .map(|g| g.members.len())
        .sum();
    stats.token_raw_exact_groups = token_raw_exact_groups.len();
    stats.token_raw_exact_units = token_raw_exact_groups.iter().map(|g| g.members.len()).sum();
    stats.line_exact_groups = line_exact_groups.len();
    stats.line_exact_units = line_exact_groups.iter().map(|g| g.members.len()).sum();
    stats
}

// ── Union-Find helpers ──────────────────────────────────────────────────

fn find(parent: &mut [usize], i: usize) -> usize {
    if parent[i] != i {
        parent[i] = find(parent, parent[i]);
    }
    parent[i]
}

fn union(parent: &mut [usize], i: usize, j: usize) {
    let ri = find(parent, i);
    let rj = find(parent, j);
    if ri != rj {
        parent[ri] = rj;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::node::{NodeKind, NormalizedNode};
    use std::path::PathBuf;

    fn test_unit(name: &str, file: &str, line_start: usize, body: NormalizedNode) -> CodeUnit {
        let fingerprint = Fingerprint::from_node(&body);
        CodeUnit {
            suppressed: None,
            parent_chain: None,
            kind: CodeUnitKind::Function,
            name: name.to_string(),
            file: PathBuf::from(file),
            line_start,
            line_end: line_start + 2,
            signature: NormalizedNode::leaf(NodeKind::Opaque),
            node_count: crate::node::count_nodes(&body),
            body,
            fingerprint,
            parent_name: None,
            is_test: false,
        }
    }

    fn token_body(value: &str) -> NormalizedNode {
        NormalizedNode::with_children(
            NodeKind::Block,
            vec![NormalizedNode::leaf(NodeKind::Token(value.to_string()))],
        )
    }

    #[test]
    fn empty_input_no_groups() {
        let groups = group_exact_duplicates(&[]);
        assert!(groups.is_empty());
    }

    // jscpd:ignore-start

    #[test]
    fn exact_group_fingerprint_ignores_locations() {
        let first = vec![
            test_unit("a", "src/a.rs", 1, token_body("same")),
            test_unit("b", "src/b.rs", 10, token_body("same")),
        ];
        let second = vec![
            test_unit("a", "renamed/a.rs", 100, token_body("same")),
            test_unit("b", "renamed/b.rs", 200, token_body("same")),
        ];

        let first_group = group_exact_duplicates_for(&first, DetectionDimension::Ast);
        let second_group = group_exact_duplicates_for(&second, DetectionDimension::Ast);

        assert_eq!(first_group.len(), 1);
        assert_eq!(second_group.len(), 1);
        assert_eq!(first_group[0].fingerprint, second_group[0].fingerprint);
    }

    #[test]
    fn near_group_fingerprint_ignores_locations() {
        let first = vec![
            test_unit("a", "src/a.rs", 1, token_body("left")),
            test_unit("b", "src/b.rs", 10, token_body("right")),
            test_unit("c", "src/c.rs", 20, token_body("middle")),
        ];
        let second = vec![
            test_unit("a", "renamed/a.rs", 100, token_body("left")),
            test_unit("b", "renamed/b.rs", 200, token_body("right")),
            test_unit("c", "renamed/c.rs", 300, token_body("middle")),
        ];

        let first_group = find_near_duplicates_for(&first, 0.0, &[], DetectionDimension::Ast);
        let second_group = find_near_duplicates_for(&second, 0.0, &[], DetectionDimension::Ast);

        assert_eq!(first_group.len(), 1);
        assert_eq!(second_group.len(), 1);
        assert_eq!(first_group[0].fingerprint, second_group[0].fingerprint);
    }

    // jscpd:ignore-end

    #[test]
    fn percentage_helpers() {
        let stats = DuplicationStats {
            total_code_units: 10,
            total_lines: 200,
            exact_duplicate_groups: 2,
            exact_duplicate_units: 4,
            near_duplicate_groups: 1,
            near_duplicate_units: 3,
            exact_duplicate_lines: 50,
            near_duplicate_lines: 30,
            sub_exact_groups: 0,
            sub_exact_units: 0,
            sub_near_groups: 0,
            sub_near_units: 0,
            ..Default::default()
        };
        assert!((stats.exact_duplicate_percent() - 25.0).abs() < f64::EPSILON);
        assert!((stats.near_duplicate_percent() - 15.0).abs() < f64::EPSILON);
    }

    #[test]
    fn percentage_helpers_zero_total() {
        let stats = DuplicationStats {
            total_code_units: 0,
            total_lines: 0,
            exact_duplicate_groups: 0,
            exact_duplicate_units: 0,
            near_duplicate_groups: 0,
            near_duplicate_units: 0,
            exact_duplicate_lines: 0,
            near_duplicate_lines: 0,
            sub_exact_groups: 0,
            sub_exact_units: 0,
            sub_near_groups: 0,
            sub_near_units: 0,
            ..Default::default()
        };
        assert!((stats.exact_duplicate_percent() - 0.0).abs() < f64::EPSILON);
        assert!((stats.near_duplicate_percent() - 0.0).abs() < f64::EPSILON);
    }
}
