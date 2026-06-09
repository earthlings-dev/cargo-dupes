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
pub mod scanner;
pub mod similarity;
pub mod text_units;

use std::collections::HashSet;
use std::path::PathBuf;

use analyzer::LanguageAnalyzer;
use code_unit::{CodeUnit, DetectionDimension};
use config::Config;
use fingerprint::Fingerprint;
use grouper::{DuplicateGroup, DuplicationStats};

/// The result of a full analysis run.
pub struct AnalysisResult {
    pub stats: DuplicationStats,
    pub exact_groups: Vec<DuplicateGroup>,
    pub near_groups: Vec<DuplicateGroup>,
    pub sub_exact_groups: Vec<DuplicateGroup>,
    pub sub_near_groups: Vec<DuplicateGroup>,
    pub token_normalized_exact_groups: Vec<DuplicateGroup>,
    pub token_normalized_near_groups: Vec<DuplicateGroup>,
    pub token_raw_exact_groups: Vec<DuplicateGroup>,
    pub line_exact_groups: Vec<DuplicateGroup>,
    pub warnings: Vec<String>,
    /// All group fingerprints (exact + near) before ignore filtering.
    /// Used by the cleanup command to identify stale ignore entries.
    pub all_fingerprints: HashSet<Fingerprint>,
}

impl AnalysisResult {
    /// Iterate over all filtered duplicate groups.
    pub fn groups(&self) -> impl Iterator<Item = &DuplicateGroup> {
        self.exact_groups
            .iter()
            .chain(self.near_groups.iter())
            .chain(self.sub_exact_groups.iter())
            .chain(self.sub_near_groups.iter())
            .chain(self.token_normalized_exact_groups.iter())
            .chain(self.token_normalized_near_groups.iter())
            .chain(self.token_raw_exact_groups.iter())
            .chain(self.line_exact_groups.iter())
    }
}

/// Run the full analysis pipeline using a language analyzer.
///
/// Reads each file, parses it via the analyzer, optionally filters test code,
/// then delegates to [`analyze_units`] for grouping, similarity, and stats.
pub fn analyze(
    analyzer: &dyn LanguageAnalyzer,
    files: &[PathBuf],
    config: &Config,
) -> error::Result<AnalysisResult> {
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
    let mut warnings = Vec::new();

    for path in ast_files {
        let source = match std::fs::read_to_string(path) {
            Ok(s) => s,
            Err(e) => {
                warnings.push(format!("Failed to read {}: {}", path.display(), e));
                continue;
            }
        };
        match analyzer.parse_file(path, &source, &analysis_config) {
            Ok(mut file_units) => {
                if config.exclude_tests {
                    file_units.retain(|u| !analyzer.is_test_code(u));
                }
                units.extend(file_units);
                if config.sub_function && config.dimension_enabled(DetectionDimension::SubAst) {
                    match analyzer.parse_sub_units(
                        path,
                        &source,
                        &analysis_config,
                        config.min_sub_nodes,
                    ) {
                        Ok(mut file_sub_units) => {
                            if config.exclude_tests {
                                file_sub_units.retain(|u| !analyzer.is_test_code(u));
                            }
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
        let source = match std::fs::read_to_string(path) {
            Ok(s) => s,
            Err(e) => {
                warnings.push(format!("Failed to read {}: {}", path.display(), e));
                continue;
            }
        };
        let generic = text_units::extract(path, &source, config);
        token_normalized_units.extend(generic.normalized_tokens);
        token_raw_units.extend(generic.raw_tokens);
        line_units.extend(generic.lines);
    }

    analyze_units_with_generic(
        &units,
        &explicit_sub_units,
        &token_normalized_units,
        &token_raw_units,
        &line_units,
        warnings,
        config,
    )
}

/// Run the analysis pipeline on pre-parsed code units.
///
/// The caller is responsible for scanning files and parsing them into `CodeUnit`s.
/// This function handles grouping, similarity detection, ignore filtering, and stats.
pub fn analyze_units(
    units: &[CodeUnit],
    warnings: Vec<String>,
    config: &Config,
) -> error::Result<AnalysisResult> {
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
    let ast_groups = compute_ast_groups(units, config);
    let sub_groups = compute_sub_ast_groups(units, explicit_sub_units, config);
    let generic_groups =
        compute_generic_groups(token_normalized_units, token_raw_units, line_units, config);

    let all_fingerprints = collect_group_fingerprints(&ast_groups, &sub_groups, &generic_groups);

    let ignore_file = ignore::load_ignore_file(&config.root);
    let ast_groups = filter_matched_groups(ast_groups, &ignore_file);
    let sub_groups = filter_matched_groups(sub_groups, &ignore_file);
    let generic_groups = filter_generic_groups(generic_groups, &ignore_file);

    let stats = grouper::with_generic_stats(
        grouper::compute_stats_with_sub(
            units,
            &ast_groups.exact,
            &ast_groups.near,
            &sub_groups.exact,
            &sub_groups.near,
        ),
        &generic_groups.token_normalized.exact,
        &generic_groups.token_normalized.near,
        &generic_groups.token_raw_exact,
        &generic_groups.line_exact,
    );

    Ok(AnalysisResult {
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
    })
}

/// Exact and near groups for a dimension.
#[derive(Default)]
struct MatchedGroups {
    /// Exact duplicate groups.
    exact: Vec<DuplicateGroup>,
    /// Near duplicate groups.
    near: Vec<DuplicateGroup>,
}

/// Generic token and line duplicate groups.
#[derive(Default)]
struct GenericGroups {
    /// Normalized token exact and near groups.
    token_normalized: MatchedGroups,
    /// Raw-token exact groups.
    token_raw_exact: Vec<DuplicateGroup>,
    /// Normalized-line exact groups.
    line_exact: Vec<DuplicateGroup>,
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
) -> MatchedGroups {
    if !(config.sub_function && config.dimension_enabled(DetectionDimension::SubAst)) {
        return MatchedGroups::default();
    }

    let synthesized_sub_units;
    let sub_units = if explicit_sub_units.is_empty() {
        synthesized_sub_units = fallback_sub_units(units, config.min_sub_nodes);
        &synthesized_sub_units
    } else {
        explicit_sub_units
    };

    compute_matched_groups(
        sub_units,
        config.similarity_threshold,
        DetectionDimension::SubAst,
    )
}

/// Compute token and line duplicate groups when their dimensions are enabled.
fn compute_generic_groups(
    token_normalized_units: &[CodeUnit],
    token_raw_units: &[CodeUnit],
    line_units: &[CodeUnit],
    config: &Config,
) -> GenericGroups {
    let token_normalized = if config.dimension_enabled(DetectionDimension::TokenNormalized) {
        compute_matched_groups(
            token_normalized_units,
            config.token_similarity_threshold,
            DetectionDimension::TokenNormalized,
        )
    } else {
        MatchedGroups::default()
    };
    let token_raw_exact = if config.dimension_enabled(DetectionDimension::TokenRaw) {
        grouper::group_exact_duplicates_for(token_raw_units, DetectionDimension::TokenRaw)
    } else {
        Vec::new()
    };
    let line_exact = if config.dimension_enabled(DetectionDimension::Line) {
        grouper::group_exact_duplicates_for(line_units, DetectionDimension::Line)
    } else {
        Vec::new()
    };

    GenericGroups {
        token_normalized,
        token_raw_exact,
        line_exact,
    }
}

/// Compute exact and near groups for one unit set.
fn compute_matched_groups(
    units: &[CodeUnit],
    similarity_threshold: f64,
    dimension: DetectionDimension,
) -> MatchedGroups {
    let exact = grouper::group_exact_duplicates_for(units, dimension);
    let exact_fingerprints = member_fingerprints(&exact);
    let near = grouper::find_near_duplicates_for(
        units,
        similarity_threshold,
        &exact_fingerprints,
        dimension,
    );

    MatchedGroups { exact, near }
}

/// Extract fallback sub-units from top-level normalized AST bodies.
fn fallback_sub_units(units: &[CodeUnit], min_sub_nodes: usize) -> Vec<CodeUnit> {
    units
        .iter()
        .flat_map(|unit| {
            extractor::extract_sub_units(&unit.body, min_sub_nodes)
                .into_iter()
                .map(|sub_unit| CodeUnit {
                    kind: sub_unit.kind,
                    name: sub_unit.description,
                    file: unit.file.clone(),
                    line_start: unit.line_start,
                    line_end: unit.line_end,
                    signature: node::NormalizedNode::leaf(node::NodeKind::Opaque),
                    body: sub_unit.node.clone(),
                    fingerprint: fingerprint::Fingerprint::from_node(&sub_unit.node),
                    node_count: sub_unit.node_count,
                    parent_name: Some(unit.name.clone()),
                    is_test: unit.is_test,
                })
        })
        .collect()
}

/// Return fingerprints of all members already covered by exact groups.
fn member_fingerprints(groups: &[DuplicateGroup]) -> Vec<Fingerprint> {
    groups
        .iter()
        .flat_map(|group| group.members.iter().map(|member| member.fingerprint))
        .collect()
}

/// Collect all group fingerprints before ignore filtering.
fn collect_group_fingerprints(
    ast_groups: &MatchedGroups,
    sub_groups: &MatchedGroups,
    generic_groups: &GenericGroups,
) -> HashSet<Fingerprint> {
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
        .map(|group| group.fingerprint)
        .collect()
}

/// Apply ignore filtering to exact and near groups.
fn filter_matched_groups(groups: MatchedGroups, ignore_file: &ignore::IgnoreFile) -> MatchedGroups {
    MatchedGroups {
        exact: ignore::filter_ignored(groups.exact, ignore_file),
        near: ignore::filter_ignored(groups.near, ignore_file),
    }
}

/// Apply ignore filtering to generic groups.
fn filter_generic_groups(groups: GenericGroups, ignore_file: &ignore::IgnoreFile) -> GenericGroups {
    GenericGroups {
        token_normalized: filter_matched_groups(groups.token_normalized, ignore_file),
        token_raw_exact: ignore::filter_ignored(groups.token_raw_exact, ignore_file),
        line_exact: ignore::filter_ignored(groups.line_exact, ignore_file),
    }
}
