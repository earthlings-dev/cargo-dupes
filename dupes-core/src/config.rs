use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use serde::{Deserialize, de::DeserializeOwned};

use crate::code_unit::DetectionDimension;

/// The subset of configuration relevant to language-specific parsing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnalysisConfig {
    /// Minimum number of AST nodes for a code unit to be analyzed.
    pub min_nodes: usize,
    /// Minimum number of source lines for a code unit to be analyzed.
    pub min_lines: usize,
}

/// Configuration for a duplicate-detection run, shared by both CLIs.
#[derive(Debug, Clone)]
pub struct Config {
    /// Minimum number of AST nodes for a code unit to be analyzed.
    pub min_nodes: usize,
    /// Similarity threshold for near-duplicates (0.0 to 1.0).
    pub similarity_threshold: f64,
    /// Path patterns to exclude from scanning.
    pub exclude: Vec<String>,
    /// Exit code threshold: fail if exact duplicate count exceeds this.
    pub max_exact_duplicates: Option<usize>,
    /// Exit code threshold: fail if near duplicate count exceeds this.
    pub max_near_duplicates: Option<usize>,
    /// Exit code threshold: fail if exact duplicate percentage exceeds this.
    pub max_exact_percent: Option<f64>,
    /// Exit code threshold: fail if near duplicate percentage exceeds this.
    pub max_near_percent: Option<f64>,
    /// Minimum number of source lines for a code unit to be analyzed.
    pub min_lines: usize,
    /// Exclude test code (#[test] functions and #[cfg(test)] modules).
    pub exclude_tests: bool,
    /// Enable sub-function duplicate detection.
    pub sub_function: bool,
    /// Minimum number of AST nodes for a sub-function unit to be analyzed.
    pub min_sub_nodes: usize,
    /// Enabled duplicate detection dimensions.
    pub enabled_dimensions: BTreeSet<DetectionDimension>,
    /// Minimum number of tokens in a token window.
    pub token_min_tokens: usize,
    /// Minimum number of source lines a token window must span.
    pub token_min_lines: usize,
    /// Similarity threshold for normalized token near-duplicates.
    pub token_similarity_threshold: f64,
    /// Minimum number of lines in a line window.
    pub line_min_lines: usize,
    /// Active suppression/admission rule set.
    pub suppression: crate::suppression::SuppressionPolicy,
    /// Non-fatal warnings produced while loading configuration.
    pub load_warnings: Vec<String>,
    /// Root path to analyze.
    pub root: PathBuf,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            min_nodes: 10,
            similarity_threshold: 0.8,
            exclude: Vec::new(),
            max_exact_duplicates: None,
            max_near_duplicates: None,
            max_exact_percent: None,
            max_near_percent: None,
            min_lines: 0,
            exclude_tests: false,
            sub_function: false,
            min_sub_nodes: 5,
            enabled_dimensions: DetectionDimension::all().iter().copied().collect(),
            token_min_tokens: 50,
            token_min_lines: 2,
            token_similarity_threshold: 0.9,
            line_min_lines: 5,
            suppression: crate::suppression::SuppressionPolicy::default(),
            load_warnings: Vec::new(),
            root: PathBuf::from("."),
        }
    }
}

/// Config as stored in dupes.toml or Cargo.toml metadata.
#[derive(Debug, Deserialize, Default)]
#[serde(default)]
struct FileConfig {
    min_nodes: Option<usize>,
    similarity_threshold: Option<f64>,
    exclude: Option<Vec<String>>,
    max_exact_duplicates: Option<usize>,
    max_near_duplicates: Option<usize>,
    max_exact_percent: Option<f64>,
    max_near_percent: Option<f64>,
    min_lines: Option<usize>,
    exclude_tests: Option<bool>,
    sub_function: Option<bool>,
    min_sub_nodes: Option<usize>,
    dimensions: Option<DimensionConfig>,
    token: Option<TokenConfig>,
    line: Option<LineConfig>,
    suppress: Option<SuppressConfig>,
}

/// Optional suppression-rule toggles from file config.
#[derive(Debug, Deserialize, Default)]
#[serde(default)]
struct SuppressConfig {
    disable: Option<Vec<String>>,
    enable: Option<Vec<String>>,
}

/// Optional dimension switches from file config.
#[derive(Debug, Deserialize, Default)]
#[serde(default)]
struct DimensionConfig {
    ast: Option<bool>,
    sub_ast: Option<bool>,
    token_normalized: Option<bool>,
    token_raw: Option<bool>,
    line: Option<bool>,
}

/// Optional token settings from file config.
#[derive(Debug, Deserialize, Default)]
#[serde(default)]
struct TokenConfig {
    min_tokens: Option<usize>,
    min_lines: Option<usize>,
    similarity_threshold: Option<f64>,
}

/// Optional line settings from file config.
#[derive(Debug, Deserialize, Default)]
#[serde(default)]
struct LineConfig {
    min_lines: Option<usize>,
}

/// Cargo.toml metadata section.
#[derive(Debug, Deserialize)]
struct CargoMetadata {
    #[serde(default)]
    package: Option<CargoPackage>,
}

#[derive(Debug, Deserialize)]
struct CargoPackage {
    #[serde(default)]
    metadata: Option<CargoPackageMetadata>,
}

#[derive(Debug, Deserialize)]
struct CargoPackageMetadata {
    #[serde(default)]
    dupes: Option<FileConfig>,
}

/// Overwrite `slot` when an override value is present.
pub(crate) fn override_with<T>(slot: &mut T, value: Option<T>) {
    if let Some(value) = value {
        *slot = value;
    }
}

/// Overwrite an optional `slot` only when an override value is present.
pub(crate) fn override_option<T>(slot: &mut Option<T>, value: Option<T>) {
    override_with(slot, value.map(Some));
}

impl Config {
    /// Extract the parsing-relevant subset of the configuration.
    #[must_use]
    pub const fn analysis_config(&self) -> AnalysisConfig {
        AnalysisConfig {
            min_nodes: self.min_nodes,
            min_lines: self.min_lines,
        }
    }

    /// Load config with the following precedence:
    /// 1. CLI overrides (applied by the caller after this method)
    /// 2. dupes.toml in the project root
    /// 3. `[package.metadata.dupes]` in Cargo.toml
    /// 4. Defaults
    #[must_use]
    pub fn load(root: &Path) -> Self {
        let mut config = Self {
            root: root.to_path_buf(),
            ..Default::default()
        };

        // Try Cargo.toml metadata first (lowest priority file config)
        if let Some(dupes) = load_cargo_metadata_config(&root.join("Cargo.toml")) {
            config.apply_file_config(&dupes);
        }

        // Try dupes.toml (higher priority)
        if let Some(file_config) = read_toml_file(&root.join("dupes.toml")) {
            config.apply_file_config(&file_config);
        }

        config
    }

    fn apply_file_config(&mut self, fc: &FileConfig) {
        override_with(&mut self.min_nodes, fc.min_nodes);
        override_with(&mut self.similarity_threshold, fc.similarity_threshold);
        if let Some(ref v) = fc.exclude {
            self.exclude.clone_from(v);
        }
        override_option(&mut self.max_exact_duplicates, fc.max_exact_duplicates);
        override_option(&mut self.max_near_duplicates, fc.max_near_duplicates);
        override_option(&mut self.max_exact_percent, fc.max_exact_percent);
        override_option(&mut self.max_near_percent, fc.max_near_percent);
        override_with(&mut self.min_lines, fc.min_lines);
        override_with(&mut self.exclude_tests, fc.exclude_tests);
        override_with(&mut self.sub_function, fc.sub_function);
        override_with(&mut self.min_sub_nodes, fc.min_sub_nodes);
        if let Some(dimensions) = &fc.dimensions {
            let toggles = [
                (DetectionDimension::Ast, dimensions.ast),
                (DetectionDimension::SubAst, dimensions.sub_ast),
                (
                    DetectionDimension::TokenNormalized,
                    dimensions.token_normalized,
                ),
                (DetectionDimension::TokenRaw, dimensions.token_raw),
                (DetectionDimension::Line, dimensions.line),
            ];
            for (dimension, toggle) in toggles {
                if let Some(enabled) = toggle {
                    self.set_dimension(dimension, enabled);
                }
            }
        }
        if let Some(token) = &fc.token {
            override_with(&mut self.token_min_tokens, token.min_tokens);
            override_with(&mut self.token_min_lines, token.min_lines);
            override_with(
                &mut self.token_similarity_threshold,
                token.similarity_threshold,
            );
        }
        if let Some(line) = &fc.line
            && let Some(v) = line.min_lines
        {
            self.line_min_lines = v;
        }
        if let Some(suppress) = &fc.suppress {
            let warnings = self.suppression.apply_toggles(
                suppress.disable.as_deref().unwrap_or_default(),
                suppress.enable.as_deref().unwrap_or_default(),
            );
            self.load_warnings.extend(warnings);
        }
    }

    /// Disable a duplicate-detection dimension.
    pub fn disable_dimension(&mut self, dimension: DetectionDimension) {
        self.enabled_dimensions.remove(&dimension);
    }

    /// Enable only the provided duplicate-detection dimensions.
    pub fn enable_only_dimensions(
        &mut self,
        dimensions: impl IntoIterator<Item = DetectionDimension>,
    ) {
        self.enabled_dimensions = dimensions.into_iter().collect();
    }

    /// Return true when a duplicate-detection dimension is enabled.
    #[must_use]
    pub fn dimension_enabled(&self, dimension: DetectionDimension) -> bool {
        self.enabled_dimensions.contains(&dimension)
    }

    /// Set a duplicate-detection dimension on or off.
    fn set_dimension(&mut self, dimension: DetectionDimension, enabled: bool) {
        if enabled {
            self.enabled_dimensions.insert(dimension);
        } else {
            self.enabled_dimensions.remove(&dimension);
        }
    }
}

fn load_cargo_metadata_config(path: &Path) -> Option<FileConfig> {
    let cargo = read_toml_file::<CargoMetadata>(path)?;
    cargo.package?.metadata?.dupes
}

fn read_toml_file<T: DeserializeOwned>(path: &Path) -> Option<T> {
    if !path.exists() {
        return None;
    }
    let content = std::fs::read_to_string(path).ok()?;
    toml::from_str(&content).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn load_with_dupes_toml(contents: &str) -> Config {
        let tmp = TempDir::new().unwrap();
        write_config(&tmp, "dupes.toml", contents);
        Config::load(tmp.path())
    }

    fn write_config(tmp: &TempDir, file_name: &str, contents: &str) {
        fs::write(tmp.path().join(file_name), contents).unwrap();
    }

    #[test]
    fn suppress_table_toggles_rules_with_dupes_toml_winning() {
        use crate::suppression::RuleId;
        let tmp = TempDir::new().unwrap();
        write_config(
            &tmp,
            "Cargo.toml",
            r#"
            [package]
            name = "test"
            version = "0.1.0"
            edition = "2021"

            [package.metadata.dupes.suppress]
            disable = ["line.chain-tail", "token.low-signal"]
            "#,
        );
        write_config(
            &tmp,
            "dupes.toml",
            r#"
            [suppress]
            enable = ["line.chain-tail"]
            disable = ["sub.value-plumbing"]
            "#,
        );
        let config = Config::load(tmp.path());
        assert!(config.suppression.is_enabled(RuleId::LineChainTail));
        assert!(!config.suppression.is_enabled(RuleId::TokenLowSignal));
        assert!(!config.suppression.is_enabled(RuleId::SubValuePlumbing));
        assert!(config.load_warnings.is_empty());
    }

    #[test]
    fn unknown_suppress_rule_ids_warn_without_failing() {
        let config = load_with_dupes_toml(
            r#"
            [suppress]
            disable = ["no.such-rule"]
            "#,
        );
        assert_eq!(
            config.load_warnings,
            vec!["unknown suppression rule id: no.such-rule"]
        );
    }

    #[test]
    fn default_config() {
        let config = Config::default();
        assert_eq!(config.min_nodes, 10);
        assert!((config.similarity_threshold - 0.8).abs() < f64::EPSILON);
        assert_eq!(config.line_min_lines, 5);
        assert!(config.exclude.is_empty());
        assert!(!config.sub_function);
    }

    #[test]
    fn load_from_dupes_toml() {
        let config = load_with_dupes_toml(
            r#"
            min_nodes = 20
            similarity_threshold = 0.9
            exclude = ["tests"]
            "#,
        );
        assert_eq!(config.min_nodes, 20);
        assert!((config.similarity_threshold - 0.9).abs() < f64::EPSILON);
        assert_eq!(config.exclude, vec!["tests".to_string()]);
    }

    #[test]
    fn load_from_cargo_toml_metadata() {
        let tmp = TempDir::new().unwrap();
        write_config(
            &tmp,
            "Cargo.toml",
            r#"
            [package]
            name = "test"
            version = "0.1.0"
            edition = "2021"

            [package.metadata.dupes]
            min_nodes = 15
            similarity_threshold = 0.75
            "#,
        );
        let config = Config::load(tmp.path());
        assert_eq!(config.min_nodes, 15);
        assert!((config.similarity_threshold - 0.75).abs() < f64::EPSILON);
    }

    #[test]
    fn dupes_toml_overrides_cargo_toml() {
        let tmp = TempDir::new().unwrap();
        write_config(
            &tmp,
            "Cargo.toml",
            r#"
            [package]
            name = "test"
            version = "0.1.0"
            edition = "2021"

            [package.metadata.dupes]
            min_nodes = 15
            "#,
        );
        write_config(
            &tmp,
            "dupes.toml",
            r"
            min_nodes = 25
            ",
        );
        let config = Config::load(tmp.path());
        assert_eq!(config.min_nodes, 25);
    }

    #[test]
    fn load_no_config_files() {
        let tmp = TempDir::new().unwrap();
        let config = Config::load(tmp.path());
        assert_eq!(config.min_nodes, 10); // default
    }

    #[test]
    fn config_with_thresholds() {
        let config = load_with_dupes_toml(
            r"
            max_exact_duplicates = 0
            max_near_duplicates = 5
            ",
        );
        assert_eq!(config.max_exact_duplicates, Some(0));
        assert_eq!(config.max_near_duplicates, Some(5));
    }

    #[test]
    fn config_with_exclude_tests() {
        let config = load_with_dupes_toml(
            r"
            exclude_tests = true
            ",
        );
        assert!(config.exclude_tests);
    }

    #[test]
    fn config_with_min_lines() {
        let config = load_with_dupes_toml(
            r"
            min_lines = 5
            ",
        );
        assert_eq!(config.min_lines, 5);
    }

    #[test]
    fn config_with_percentage_thresholds() {
        let config = load_with_dupes_toml(
            r"
            max_exact_percent = 5.0
            max_near_percent = 10.5
            ",
        );
        assert_eq!(config.max_exact_percent, Some(5.0));
        assert_eq!(config.max_near_percent, Some(10.5));
    }

    #[test]
    fn config_with_token_min_lines() {
        let config = load_with_dupes_toml(
            r"
            [token]
            min_tokens = 25
            min_lines = 3
            ",
        );
        assert_eq!(config.token_min_tokens, 25);
        assert_eq!(config.token_min_lines, 3);
    }

    #[test]
    fn enable_only_dimensions_replaces_default_dimensions() {
        let mut config = Config::default();
        config.enable_only_dimensions([DetectionDimension::Line]);
        assert!(config.dimension_enabled(DetectionDimension::Line));
        assert!(!config.dimension_enabled(DetectionDimension::Ast));
        assert!(!config.dimension_enabled(DetectionDimension::TokenNormalized));
    }
}
