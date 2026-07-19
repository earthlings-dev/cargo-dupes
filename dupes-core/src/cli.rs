//! Shared CLI machinery for `cargo-dupes` and `code-dupes`: typed CLI
//! errors and exit codes, common argument/subcommand types, override
//! resolution, the analysis entry point, and the command implementations.

use std::io::Write;
use std::io::{
  self,
};
use std::path::Path;
use std::path::PathBuf;
use std::process;

use crate::AnalysisResult;
use crate::analyzer::LanguageAnalyzer;
use crate::code_unit::DetectionDimension;
use crate::config::Config;
use crate::fingerprint::Fingerprint;
use crate::grouper::DuplicateGroup;
use crate::grouper::DuplicationStats;
use crate::ignore::IgnoreEntry;
use crate::ignore::{
  self,
};
use crate::output::ReportOptions;
use crate::output::Reporter;
use crate::output::json::JsonReporter;
use crate::output::text::TextReporter;

// ---------------------------------------------------------------------------
// Error types
// ---------------------------------------------------------------------------

/// Errors returned by CLI command functions.
#[derive(Debug)]
pub enum CliError {
  /// An I/O error (exit code 2).
  Io(io::Error),
  /// No source files found (exit code 2).
  NoSourceFiles(PathBuf),
  /// No recognized source files for language auto-detection (exit code 2).
  NoRecognizedFiles,
  /// Multiple languages detected — user must specify `--language` (exit code 2).
  AmbiguousLanguage(Vec<String>),
  /// Analysis pipeline failed (exit code 2).
  Analysis(crate::error::Error),
  /// Invalid fingerprint string (exit code 2).
  InvalidFingerprint(String),
  /// Check thresholds exceeded (exit code 1).
  CheckFailed,
}

impl CliError {
  /// Map to an appropriate process exit code.
  #[must_use]
  pub const fn exit_code(&self) -> i32 {
    match self {
      Self::CheckFailed => 1,
      Self::Io(_)
      | Self::NoSourceFiles(_)
      | Self::NoRecognizedFiles
      | Self::AmbiguousLanguage(_)
      | Self::Analysis(_)
      | Self::InvalidFingerprint(_) => 2,
    }
  }
}

impl std::fmt::Display for CliError {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match self {
      Self::Io(e) => write!(f, "{e}"),
      Self::NoSourceFiles(path) => {
        write!(f, "No source files found in {}", path.display())
      }
      Self::NoRecognizedFiles => {
        write!(f, "No recognized source files found. Use --language to specify the language.")
      }
      Self::AmbiguousLanguage(langs) => {
        write!(
          f,
          "Multiple languages detected: {}. Use --language to specify which to analyze.",
          langs.join(", ")
        )
      }
      Self::Analysis(e) => write!(f, "{e}"),
      Self::InvalidFingerprint(fp) => write!(f, "Invalid fingerprint: {fp}"),
      Self::CheckFailed => write!(f, "Check failed"),
    }
  }
}

impl std::error::Error for CliError {
  fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
    match self {
      Self::Io(e) => Some(e),
      Self::Analysis(e) => Some(e),
      _ => None,
    }
  }
}

impl From<io::Error> for CliError {
  fn from(e: io::Error) -> Self {
    Self::Io(e)
  }
}

impl From<crate::error::Error> for CliError {
  fn from(e: crate::error::Error) -> Self {
    Self::Analysis(e)
  }
}

/// Result type for CLI operations.
pub type CliResult<T = ()> = Result<T, CliError>;

// ---------------------------------------------------------------------------
// Shared CLI types
// ---------------------------------------------------------------------------

/// Output format for CLI reports.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "cli", derive(clap::ValueEnum))]
pub enum OutputFormat {
  /// Human-readable text (the default).
  #[default]
  Text,
  /// Machine-readable JSON.
  Json,
}

/// Global CLI options shared by `cargo-dupes` and `code-dupes`.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "cli", derive(clap::Args))]
#[allow(clippy::struct_excessive_bools)] // clap flags are idiomatically bools
pub struct CommonCliArgs {
  /// Path to analyze (defaults to current directory).
  #[cfg_attr(feature = "cli", arg(short, long, global = true))]
  pub path:              Option<PathBuf>,
  /// Minimum AST node count for analysis.
  #[cfg_attr(feature = "cli", arg(long, global = true))]
  pub min_nodes:         Option<usize>,
  /// Minimum source line count for analysis.
  #[cfg_attr(feature = "cli", arg(long, global = true))]
  pub min_lines:         Option<usize>,
  /// Similarity threshold (0.0-1.0).
  #[cfg_attr(feature = "cli", arg(long, global = true))]
  pub threshold:         Option<f64>,
  /// Output format.
  #[cfg_attr(feature = "cli", arg(long, global = true, default_value = "text"))]
  pub format:            OutputFormat,
  /// Exclude patterns (can be repeated).
  #[cfg_attr(feature = "cli", arg(long, global = true))]
  pub exclude:           Vec<String>,
  /// Exclude test code identified by the active language analyzer.
  #[cfg_attr(feature = "cli", arg(long, global = true))]
  pub exclude_tests:     bool,
  /// Enable sub-function duplicate detection.
  #[cfg_attr(feature = "cli", arg(long, short = 's', global = true))]
  pub sub_function:      bool,
  /// Disable sub-function duplicate detection when enabled by config.
  #[cfg_attr(feature = "cli", arg(long, global = true, conflicts_with = "sub_function"))]
  pub no_sub_function:   bool,
  /// Minimum AST node count for sub-function units.
  #[cfg_attr(feature = "cli", arg(long, global = true))]
  pub min_sub_nodes:     Option<usize>,
  /// Include rule-suppressed duplicate groups in the report body.
  #[cfg_attr(feature = "cli", arg(long, global = true))]
  pub show_suppressed:   bool,
  /// Verbose statistics: per-rule suppression breakdown.
  #[cfg_attr(feature = "cli", arg(short = 'v', long, global = true))]
  pub verbose:           bool,
  /// Disable a suppression/admission rule by id (can be repeated).
  #[cfg_attr(
    feature = "cli",
    arg(long = "disable-rule", value_name = "RULE_ID", global = true)
  )]
  pub disable_rule:      Vec<String>,
  /// Enable a rule by id (can be repeated; overrides config disable).
  #[cfg_attr(
    feature = "cli",
    arg(long = "enable-rule", value_name = "RULE_ID", global = true)
  )]
  pub enable_rule:       Vec<String>,
  /// Enable only the selected detection dimension (can be repeated).
  #[cfg_attr(feature = "cli", arg(long, global = true, conflicts_with = "disable_dimension"))]
  pub dimension:         Vec<DetectionDimension>,
  /// Disable a detection dimension (can be repeated).
  #[cfg_attr(feature = "cli", arg(long, global = true, conflicts_with = "dimension"))]
  pub disable_dimension: Vec<DetectionDimension>,
  /// Minimum token count for token-window detection.
  #[cfg_attr(feature = "cli", arg(long, global = true))]
  pub token_min_tokens:  Option<usize>,
  /// Minimum source line span for token-window detection.
  #[cfg_attr(feature = "cli", arg(long, global = true))]
  pub token_min_lines:   Option<usize>,
  /// Similarity threshold for normalized token near-duplicates.
  #[cfg_attr(feature = "cli", arg(long, global = true))]
  pub token_threshold:   Option<f64>,
  /// Minimum line count for line-window detection.
  #[cfg_attr(feature = "cli", arg(long, global = true))]
  pub line_min_lines:    Option<usize>,
}

impl Default for CommonCliArgs {
  fn default() -> Self {
    Self {
      path:              None,
      min_nodes:         None,
      min_lines:         None,
      threshold:         None,
      format:            OutputFormat::Text,
      exclude:           Vec::new(),
      exclude_tests:     false,
      sub_function:      false,
      no_sub_function:   false,
      min_sub_nodes:     None,
      show_suppressed:   false,
      verbose:           false,
      disable_rule:      Vec::new(),
      enable_rule:       Vec::new(),
      dimension:         Vec::new(),
      disable_dimension: Vec::new(),
      token_min_tokens:  None,
      token_min_lines:   None,
      token_threshold:   None,
      line_min_lines:    None,
    }
  }
}

impl CommonCliArgs {
  /// Resolve the analysis root from CLI input.
  #[must_use]
  pub fn root(&self) -> PathBuf {
    self
      .path
      .clone()
      .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
  }

  /// Convert common CLI options into shared core overrides.
  #[must_use]
  pub fn overrides(&self, generic_extensions: Vec<String>) -> CliOverrides {
    CliOverrides {
      min_nodes: self.min_nodes,
      min_lines: self.min_lines,
      threshold: self.threshold,
      exclude: self.exclude.clone(),
      exclude_tests: if self.exclude_tests { Some(true) } else { None },
      sub_function: if self.no_sub_function {
        Some(false)
      } else if self.sub_function {
        Some(true)
      } else {
        None
      },
      min_sub_nodes: self.min_sub_nodes,
      enabled_dimensions: self.dimension.clone(),
      disabled_dimensions: self.disable_dimension.clone(),
      token_min_tokens: self.token_min_tokens,
      token_min_lines: self.token_min_lines,
      token_threshold: self.token_threshold,
      line_min_lines: self.line_min_lines,
      show_suppressed: self.show_suppressed,
      verbose: self.verbose,
      disable_rule: self.disable_rule.clone(),
      enable_rule: self.enable_rule.clone(),
      generic_extensions,
    }
  }
}

/// CLI subcommands shared between `cargo-dupes` and `code-dupes`.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "cli", derive(clap::Subcommand))]
pub enum Command {
  /// Show duplication statistics only.
  Stats,
  /// Show full duplication report (default).
  Report,
  /// Check for duplicates and exit with non-zero if thresholds exceeded.
  Check {
    /// Maximum allowed exact duplicate groups (exit 1 if exceeded).
    #[cfg_attr(feature = "cli", arg(long))]
    max_exact:         Option<usize>,
    /// Maximum allowed near duplicate groups (exit 1 if exceeded).
    #[cfg_attr(feature = "cli", arg(long))]
    max_near:          Option<usize>,
    /// Maximum allowed exact duplicate percentage (exit 1 if exceeded).
    #[cfg_attr(feature = "cli", arg(long))]
    max_exact_percent: Option<f64>,
    /// Maximum allowed near duplicate percentage (exit 1 if exceeded).
    #[cfg_attr(feature = "cli", arg(long))]
    max_near_percent:  Option<f64>,
  },
  /// Add a fingerprint to the ignore list.
  Ignore {
    /// The fingerprint to ignore (hex string).
    fingerprint: String,
    /// Reason for ignoring.
    #[cfg_attr(feature = "cli", arg(long))]
    reason:      Option<String>,
  },
  /// List all ignored fingerprints.
  Ignored,
  /// Remove stale entries from the ignore file.
  Cleanup {
    /// Only list stale entries without removing them.
    #[cfg_attr(feature = "cli", arg(long))]
    dry_run: bool,
  },
}

impl Command {
  /// Build threshold overrides for `check` commands.
  #[must_use]
  pub const fn check_thresholds(&self) -> Option<CheckThresholds> {
    match self {
      Self::Check {
        max_exact,
        max_near,
        max_exact_percent,
        max_near_percent,
      } => Some(CheckThresholds {
        max_exact:         *max_exact,
        max_near:          *max_near,
        max_exact_percent: *max_exact_percent,
        max_near_percent:  *max_near_percent,
      }),
      _ => None,
    }
  }
}

/// Thresholds for the `check` subcommand.
#[derive(Debug, Clone, Default)]
pub struct CheckThresholds {
  /// Maximum allowed exact duplicate groups.
  pub max_exact:         Option<usize>,
  /// Maximum allowed near duplicate groups.
  pub max_near:          Option<usize>,
  /// Maximum allowed exact duplicate-line percentage.
  pub max_exact_percent: Option<f64>,
  /// Maximum allowed near duplicate-line percentage.
  pub max_near_percent:  Option<f64>,
}

/// Optional CLI overrides applied on top of file-based config.
#[derive(Debug, Clone, Default)]
pub struct CliOverrides {
  /// Minimum AST node count for analysis.
  pub min_nodes:           Option<usize>,
  /// Minimum source line count for analysis.
  pub min_lines:           Option<usize>,
  /// Similarity threshold for near-duplicates.
  pub threshold:           Option<f64>,
  /// Additional path patterns to exclude from scanning.
  pub exclude:             Vec<String>,
  /// Exclude test code identified by the language analyzer.
  pub exclude_tests:       Option<bool>,
  /// Enable or disable sub-function duplicate detection.
  pub sub_function:        Option<bool>,
  /// Minimum AST node count for sub-function units.
  pub min_sub_nodes:       Option<usize>,
  /// When nonempty, enable only these detection dimensions.
  pub enabled_dimensions:  Vec<DetectionDimension>,
  /// Detection dimensions to disable.
  pub disabled_dimensions: Vec<DetectionDimension>,
  /// Minimum token count for token-window detection.
  pub token_min_tokens:    Option<usize>,
  /// Minimum source line span for token-window detection.
  pub token_min_lines:     Option<usize>,
  /// Similarity threshold for normalized token near-duplicates.
  pub token_threshold:     Option<f64>,
  /// Minimum line count for line-window detection.
  pub line_min_lines:      Option<usize>,
  /// Include rule-suppressed duplicate groups in the report body.
  pub show_suppressed:     bool,
  /// Verbose statistics: per-rule suppression breakdown.
  pub verbose:             bool,
  /// Suppression/admission rule ids to disable.
  pub disable_rule:        Vec<String>,
  /// Rule ids to enable (overriding config disables).
  pub enable_rule:         Vec<String>,
  /// File extensions scanned by the generic token/line dimensions.
  pub generic_extensions:  Vec<String>,
}

/// Result of [`run_analysis`].
pub struct AnalysisOutput {
  /// The fully resolved configuration the analysis ran with.
  pub config:   Config,
  /// The analysis findings.
  pub result:   AnalysisResult,
  /// Reporter matching the requested output format.
  pub reporter: Box<dyn Reporter>,
}

// ---------------------------------------------------------------------------
// Command orchestration
// ---------------------------------------------------------------------------

/// Run a CLI command, invoking `analyze` only for commands that need analysis.
pub fn run_command_with_analysis(
  root: &Path,
  command: &Command,
  writer: &mut impl Write,
  analyze: impl FnOnce() -> CliResult<AnalysisOutput>,
) -> CliResult {
  if matches!(command, Command::Ignored) {
    return cmd_ignored(root, writer);
  }
  let output = analyze()?;
  emit_warnings(&output.result);
  dispatch_analysis_command(root, command, &output, writer)
}

/// Dispatch a command after analysis has completed.
pub fn dispatch_analysis_command(root: &Path, command: &Command, output: &AnalysisOutput, writer: &mut impl Write) -> CliResult {
  let reporter = output.reporter.as_ref();

  match command {
    Command::Stats => cmd_stats(&output.result, reporter, writer),
    Command::Report => cmd_report(&output.result, reporter, writer),
    Command::Check {
      ..
    } => cmd_check(
      &output.config,
      &output.result,
      reporter,
      writer,
      &command.check_thresholds().expect("check thresholds exist for check command"),
    ),
    Command::Cleanup {
      dry_run,
    } => cmd_cleanup(root, &output.result, writer, *dry_run),
    Command::Ignore {
      fingerprint,
      reason,
    } => cmd_ignore(root, fingerprint, reason.clone(), &output.result, writer),
    Command::Ignored => Ok(()),
  }
}

/// Write analysis warnings using the shared CLI format.
pub fn emit_warnings(result: &AnalysisResult) {
  for warning in &result.warnings {
    eprintln!("Warning: {warning}");
  }
}

/// Exit with the CLI's documented exit code.
pub fn exit_with_error(error: &CliError) -> ! {
  if !matches!(error, CliError::CheckFailed) {
    eprintln!("Error: {error}");
  }
  process::exit(error.exit_code());
}

// ---------------------------------------------------------------------------
// Config helpers
// ---------------------------------------------------------------------------

/// Apply CLI overrides to a loaded `Config`.
///
/// CLI `--exclude` patterns are *appended* to config-file excludes (not replaced).
pub fn apply_overrides(config: &mut Config, overrides: &CliOverrides) {
  use crate::config::override_with;
  override_with(&mut config.min_nodes, overrides.min_nodes);
  override_with(&mut config.min_lines, overrides.min_lines);
  override_with(&mut config.similarity_threshold, overrides.threshold);
  if !overrides.exclude.is_empty() {
    config.exclude.extend(overrides.exclude.iter().cloned());
  }
  override_with(&mut config.exclude_tests, overrides.exclude_tests);
  override_with(&mut config.sub_function, overrides.sub_function);
  override_with(&mut config.min_sub_nodes, overrides.min_sub_nodes);
  if !overrides.enabled_dimensions.is_empty() {
    config.enable_only_dimensions(overrides.enabled_dimensions.iter().copied());
  }
  for &dimension in &overrides.disabled_dimensions {
    config.disable_dimension(dimension);
  }
  let rule_warnings = config
    .suppression
    .apply_toggles(&overrides.disable_rule, &overrides.enable_rule);
  config.load_warnings.extend(rule_warnings);
  override_with(&mut config.token_min_tokens, overrides.token_min_tokens);
  override_with(&mut config.token_min_lines, overrides.token_min_lines);
  override_with(&mut config.token_similarity_threshold, overrides.token_threshold);
  override_with(&mut config.line_min_lines, overrides.line_min_lines);
}

/// Create a reporter for the given output format.
#[must_use]
pub fn create_reporter(format: OutputFormat, root: Option<&Path>, options: ReportOptions) -> Box<dyn Reporter> {
  match format {
    OutputFormat::Text => Box::new(TextReporter::with_options(root.map(Path::to_path_buf), options)),
    OutputFormat::Json => Box::new(JsonReporter::with_options(root.map(Path::to_path_buf), options)),
  }
}

// ---------------------------------------------------------------------------
// Analysis
// ---------------------------------------------------------------------------

/// Scan files, run the analysis pipeline, and return the output.
///
/// Warnings are stored in [`AnalysisOutput::result`] but **not** printed;
/// the caller is responsible for writing them to stderr.
pub fn run_analysis(
  analyzer: &dyn LanguageAnalyzer,
  root: &Path,
  format: OutputFormat,
  overrides: &CliOverrides,
) -> CliResult<AnalysisOutput> {
  let mut config = Config::load(root);
  apply_overrides(&mut config, overrides);

  let scan_config = crate::scanner::ScanConfig::new(config.root.clone())
    .with_excludes(config.exclude.clone())
    .with_extensions(
      analyzer
        .file_extensions()
        .iter()
        .map(std::string::ToString::to_string)
        .collect(),
    );
  let ast_files = crate::scanner::scan_files(&scan_config);

  let generic_files = if config.dimension_enabled(DetectionDimension::TokenNormalized)
    || config.dimension_enabled(DetectionDimension::TokenRaw)
    || config.dimension_enabled(DetectionDimension::Line)
  {
    let generic_extensions = if overrides.generic_extensions.is_empty() {
      analyzer
        .file_extensions()
        .iter()
        .map(std::string::ToString::to_string)
        .collect()
    } else {
      overrides.generic_extensions.clone()
    };
    let generic_scan_config = crate::scanner::ScanConfig::new(config.root.clone())
      .with_excludes(config.exclude.clone())
      .with_extensions(generic_extensions);
    crate::scanner::scan_files(&generic_scan_config)
  } else {
    Vec::new()
  };

  if ast_files.is_empty() && generic_files.is_empty() {
    return Err(CliError::NoSourceFiles(config.root));
  }

  let result = crate::analyze_with_generic(analyzer, &ast_files, &generic_files, &config)?;
  let reporter = create_reporter(format, Some(root), ReportOptions {
    show_suppressed: overrides.show_suppressed,
    verbose:         overrides.verbose,
  });

  Ok(AnalysisOutput {
    config,
    result,
    reporter,
  })
}

// ---------------------------------------------------------------------------
// Command implementations
// ---------------------------------------------------------------------------

/// Show duplication statistics only.
pub fn cmd_stats(result: &AnalysisResult, reporter: &dyn Reporter, writer: &mut impl Write) -> CliResult {
  reporter.report_stats(&result.stats, writer)?;
  Ok(())
}

/// Show a full duplication report (stats + groups).
pub fn cmd_report(result: &AnalysisResult, reporter: &dyn Reporter, writer: &mut impl Write) -> CliResult {
  reporter.report_full(result, writer)?;
  Ok(())
}

/// Check thresholds; returns `Err(CliError::CheckFailed)` if any are exceeded.
pub fn cmd_check(
  config: &Config,
  result: &AnalysisResult,
  reporter: &dyn Reporter,
  writer: &mut impl Write,
  thresholds: &CheckThresholds,
) -> CliResult {
  let resolved = CheckThresholds {
    max_exact:         thresholds.max_exact.or(config.max_exact_duplicates),
    max_near:          thresholds.max_near.or(config.max_near_duplicates),
    max_exact_percent: thresholds.max_exact_percent.or(config.max_exact_percent),
    max_near_percent:  thresholds.max_near_percent.or(config.max_near_percent),
  };
  let outcome = evaluate_check(&resolved, &result.stats);

  if outcome.passed {
    reporter.report_stats(&result.stats, writer)?;
    writeln!(writer, "\nCheck passed.")?;
    Ok(())
  } else {
    // Render stats and the grouped report exactly once, then summarize
    // every tripped gate. Evaluation is separated from rendering so the
    // report is never re-emitted per gate.
    reporter.report_full(result, writer)?;
    for breach in &outcome.failed_gates {
      writeln!(writer, "\nCheck FAILED: {}", breach.summary())?;
    }
    Err(CliError::CheckFailed)
  }
}

/// Add a fingerprint to the ignore list.
pub fn cmd_ignore(root: &Path, fingerprint: &str, reason: Option<String>, result: &AnalysisResult, writer: &mut impl Write) -> CliResult {
  let fp = Fingerprint::from_hex(fingerprint).ok_or_else(|| CliError::InvalidFingerprint(fingerprint.to_string()))?;
  let mut ignore_file = ignore::load_ignore_file(root);
  // Rule-suppressed groups can be registered too, so a project can pin a
  // finding before disabling the rule that hides it.
  let group = result.groups_with_suppressed().find(|group| group.fingerprint == fp);
  let (members, member_fingerprints) = group.map_or_else(Default::default, |group| {
    (
      group.members.iter().map(|member| display_member(root, member)).collect(),
      member_fingerprint_hexes(group),
    )
  });
  ignore::add_ignore_with_member_fingerprints(&mut ignore_file, &fp, reason, members, member_fingerprints);
  ignore::save_ignore_file(root, &ignore_file)?;
  writeln!(writer, "Added {fingerprint} to ignore list.")?;
  if group.is_none() {
    writeln!(
      writer,
      "Note: no current duplicate group matches this fingerprint; the entry was recorded without member details."
    )?;
  }
  Ok(())
}

/// Format one group member for ignore-entry documentation.
fn display_member(root: &Path, member: &crate::code_unit::CodeUnit) -> String {
  format!(
    "{} ({}:{}-{})",
    member.name,
    crate::output::display_path(Some(root), &member.file),
    member.line_start,
    member.line_end
  )
}

/// The sorted, deduplicated member content fingerprints of a group.
fn member_fingerprint_hexes(group: &DuplicateGroup) -> Vec<String> {
  let mut hexes: Vec<String> = group.members.iter().map(|member| member.fingerprint.to_hex()).collect();
  hexes.sort_unstable();
  hexes.dedup();
  hexes
}

/// List all ignored fingerprints.
pub fn cmd_ignored(root: &Path, writer: &mut impl Write) -> CliResult {
  let ignore_file = ignore::load_ignore_file(root);
  if ignore_file.ignore.is_empty() {
    writeln!(writer, "No ignored fingerprints.")?;
  } else {
    write_ignore_entries(writer, "Ignored fingerprints:", &ignore_file.ignore)?;
  }
  Ok(())
}

/// Remove stale entries from the ignore file.
pub fn cmd_cleanup(root: &Path, result: &AnalysisResult, writer: &mut impl Write, dry_run: bool) -> CliResult {
  let mut ignore_file = ignore::load_ignore_file(root);

  if dry_run {
    let stale = ignore::find_stale_entries(&ignore_file, &result.all_fingerprints, &result.all_member_fingerprint_sets);
    if stale.is_empty() {
      writeln!(writer, "No stale entries found.")?;
    } else {
      writeln!(writer, "Stale entries (dry run):")?;
      for entry in &stale {
        write_ignore_entry(writer, entry)?;
        write_successor_hints(writer, root, entry, result)?;
      }
      writeln!(writer, "\n{} stale entries would be removed.", stale.len())?;
    }
  } else {
    let removed = ignore::remove_stale_entries(&mut ignore_file, &result.all_fingerprints, &result.all_member_fingerprint_sets);
    if removed.is_empty() {
      writeln!(writer, "No stale entries found.")?;
    } else {
      ignore::save_ignore_file(root, &ignore_file)?;
      write_ignore_entries(writer, "Removed stale entries:", &removed)?;
      writeln!(writer, "\nRemoved {} stale entries.", removed.len())?;
    }
  }
  Ok(())
}

/// Suggest live groups that overlap a stale entry's recorded members.
///
/// When registered content is edited, the duplicate family usually survives
/// with a new fingerprint over shifted content. Pointing at live groups that
/// overlap the entry's recorded locations turns registry repair into a
/// guided rewrite instead of a search.
fn write_successor_hints(writer: &mut impl Write, root: &Path, entry: &IgnoreEntry, result: &AnalysisResult) -> io::Result<()> {
  let locations = member_locations(entry);
  if locations.is_empty() {
    return Ok(());
  }
  // Suppressed groups count as successors: a stale entry can be re-paired
  // to a finding the rules hide from the default report.
  for group in result.groups_with_suppressed() {
    let overlapping = group.members.iter().any(|member| {
      let member_path = crate::output::display_path(Some(root), &member.file);
      locations.iter().any(|(path, start, end)| {
        member_path.ends_with(path.as_str())
          && member.line_start <= end.saturating_add(SUCCESSOR_LINE_SLACK)
          && *start <= member.line_end.saturating_add(SUCCESSOR_LINE_SLACK)
      })
    });
    if overlapping {
      writeln!(
        writer,
        "    possible successor: {} ({}/{}, {} members)",
        group.fingerprint,
        group.dimension,
        group.match_kind,
        group.members.len()
      )?;
    }
  }
  Ok(())
}

/// Lines of drift tolerated when pairing stale entries with successors.
const SUCCESSOR_LINE_SLACK: usize = 5;

/// Parse `path:start-end` locations out of an entry's member descriptions.
fn member_locations(entry: &IgnoreEntry) -> Vec<(String, usize, usize)> {
  entry
    .members
    .iter()
    .flat_map(|member| member.split([' ', '(', ')', '[', ']', ',']))
    .filter_map(|token| {
      let (path, range) = token.rsplit_once(':')?;
      let (start, end) = range.split_once('-')?;
      if path.is_empty() {
        return None;
      }
      Some((path.to_string(), start.parse().ok()?, end.parse().ok()?))
    })
    .collect()
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Write a titled list of ignore entries.
fn write_ignore_entries<'a>(writer: &mut impl Write, title: &str, entries: impl IntoIterator<Item = &'a IgnoreEntry>) -> io::Result<()> {
  writeln!(writer, "{title}")?;
  for entry in entries {
    write_ignore_entry(writer, entry)?;
  }
  Ok(())
}

/// A duplicate-detection dimension that a `check` gate guards.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GateDimension {
  Exact,
  Near,
}

impl GateDimension {
  const fn label(self) -> &'static str {
    match self {
      Self::Exact => "exact",
      Self::Near => "near",
    }
  }
}

/// One `check` threshold that was exceeded.
#[derive(Debug, Clone, PartialEq)]
enum GateBreach {
  Count {
    dimension: GateDimension,
    observed:  usize,
    limit:     usize,
  },
  Percent {
    dimension: GateDimension,
    observed:  f64,
    limit:     f64,
  },
}

impl GateBreach {
  /// Failure summary line, without the leading `Check FAILED: ` prefix.
  fn summary(&self) -> String {
    match *self {
      Self::Count {
        dimension,
        observed,
        limit,
      } => {
        let label = dimension.label();
        format!("{observed} {label} duplicate groups (max: {limit})")
      }
      Self::Percent {
        dimension,
        observed,
        limit,
      } => {
        let label = dimension.label();
        format!("{observed:.1}% {label} duplicate lines (max: {limit:.1}%)")
      }
    }
  }
}

/// The outcome of a `check`: whether it passed, and every gate it tripped.
#[derive(Debug, Clone, PartialEq)]
struct CheckOutcome {
  passed:       bool,
  failed_gates: Vec<GateBreach>,
}

/// Evaluate every `check` gate against the stats, collecting all breaches.
///
/// Pure: gate evaluation is kept separate from rendering so the report is
/// emitted exactly once regardless of how many gates trip.
fn evaluate_check(thresholds: &CheckThresholds, stats: &DuplicationStats) -> CheckOutcome {
  let exact_group_count = stats.exact_duplicate_groups
    + stats.sub_exact_groups
    + stats.token_normalized_exact_groups
    + stats.token_raw_exact_groups
    + stats.line_exact_groups;
  let near_group_count = stats.near_duplicate_groups + stats.sub_near_groups + stats.token_normalized_near_groups;

  let failed_gates: Vec<GateBreach> = [
    exceeded(thresholds.max_exact, exact_group_count).map(|(observed, limit)| GateBreach::Count {
      dimension: GateDimension::Exact,
      observed,
      limit,
    }),
    exceeded(thresholds.max_near, near_group_count).map(|(observed, limit)| GateBreach::Count {
      dimension: GateDimension::Near,
      observed,
      limit,
    }),
    exceeded(thresholds.max_exact_percent, stats.exact_duplicate_percent()).map(|(observed, limit)| GateBreach::Percent {
      dimension: GateDimension::Exact,
      observed,
      limit,
    }),
    exceeded(thresholds.max_near_percent, stats.near_duplicate_percent()).map(|(observed, limit)| GateBreach::Percent {
      dimension: GateDimension::Near,
      observed,
      limit,
    }),
  ]
  .into_iter()
  .flatten()
  .collect();

  CheckOutcome {
    passed: failed_gates.is_empty(),
    failed_gates,
  }
}

/// The `(observed, limit)` pair when `observed` exceeds an optional limit, or
/// `None` when the gate is unset or within budget. One generic gate test keeps
/// the count and percentage dimensions from diverging into parallel helpers.
fn exceeded<T: Copy + PartialOrd>(limit: Option<T>, observed: T) -> Option<(T, T)> {
  match limit {
    Some(limit) if observed > limit => Some((observed, limit)),
    _ => None,
  }
}

fn write_ignore_entry(writer: &mut impl Write, entry: &IgnoreEntry) -> io::Result<()> {
  write!(writer, "  {}", entry.fingerprint)?;
  if let Some(reason) = &entry.reason {
    write!(writer, " (reason: {reason})")?;
  }
  if !entry.members.is_empty() {
    write!(writer, " [{}]", entry.members.join(", "))?;
  }
  writeln!(writer)
}

#[cfg(test)]
mod tests {
  use tempfile::TempDir;

  use super::*;
  use crate::code_unit::CodeUnit;
  use crate::code_unit::CodeUnitKind;
  use crate::code_unit::DetectionDimension;
  use crate::grouper::DuplicationStats;
  use crate::grouper::MatchKind;
  use crate::output::ReportSection;
  use crate::output::test_support;

  fn window_member(seed: &str, file: &str, line_start: usize, line_end: usize) -> CodeUnit {
    crate::text_units::window_unit(Path::new(file), "line window", CodeUnitKind::LineWindow, line_start, line_end, &[
      seed.to_string(),
    ])
  }

  fn empty_result() -> AnalysisResult {
    test_support::analysis_result(DuplicationStats::default(), Vec::new(), Vec::new(), Vec::new())
  }

  fn result_with_line_group(group: DuplicateGroup) -> AnalysisResult {
    let mut result = empty_result();
    result.line_exact_groups = vec![group];
    result
  }

  fn stats_with_groups(exact_groups: usize, near_groups: usize) -> DuplicationStats {
    DuplicationStats {
      exact_duplicate_groups: exact_groups,
      near_duplicate_groups: near_groups,
      ..Default::default()
    }
  }

  /// Reporter that counts full-report renders and rejects the render paths a
  /// failing `check` must never take, so a test can assert the failure path
  /// renders exactly once instead of re-rendering per tripped gate. The three
  /// methods stay structurally distinct so they do not register as twins.
  #[derive(Default)]
  struct CountingReporter {
    full_renders: std::cell::Cell<usize>,
  }

  impl Reporter for CountingReporter {
    fn report_full(&self, _result: &AnalysisResult, _writer: &mut dyn std::io::Write) -> std::io::Result<()> {
      self.full_renders.set(self.full_renders.get() + 1);
      Ok(())
    }

    fn report_stats(&self, _stats: &DuplicationStats, _writer: &mut dyn std::io::Write) -> std::io::Result<()> {
      panic!("a failing check renders the full report, never bare stats")
    }

    fn report_groups(&self, _groups: &[DuplicateGroup], _writer: &mut dyn std::io::Write, _section: ReportSection) -> std::io::Result<()> {
      unreachable!("cmd_check never renders an individual section")
    }
  }

  #[test]
  fn evaluate_check_collects_each_breached_gate_once() {
    let thresholds = CheckThresholds {
      max_exact: Some(0),
      max_near: Some(0),
      ..Default::default()
    };
    assert_eq!(evaluate_check(&thresholds, &stats_with_groups(1, 1)), CheckOutcome {
      passed:       false,
      failed_gates: vec![
        GateBreach::Count {
          dimension: GateDimension::Exact,
          observed:  1,
          limit:     0,
        },
        GateBreach::Count {
          dimension: GateDimension::Near,
          observed:  1,
          limit:     0,
        },
      ],
    });
  }

  #[test]
  fn evaluate_check_passes_within_thresholds() {
    assert_eq!(
      evaluate_check(&CheckThresholds::default(), &stats_with_groups(1, 1)),
      CheckOutcome {
        passed:       true,
        failed_gates: vec![],
      }
    );
  }

  #[test]
  fn cmd_check_renders_once_on_multi_gate_failure() {
    let result = test_support::analysis_result(stats_with_groups(1, 1), Vec::new(), Vec::new(), Vec::new());
    let reporter = CountingReporter::default();
    let mut sink = Vec::new();
    let thresholds = CheckThresholds {
      max_exact: Some(0),
      max_near: Some(0),
      ..Default::default()
    };

    let err = cmd_check(&Config::default(), &result, &reporter, &mut sink, &thresholds).expect_err("two tripped gates must fail the check");

    assert!(matches!(err, CliError::CheckFailed));
    // Exactly one render call — the bug re-rendered stats + report per gate.
    assert_eq!(reporter.full_renders.get(), 1);
  }

  #[test]
  fn cmd_ignore_records_member_fingerprints_from_the_live_group() {
    let tmp = TempDir::new().unwrap();
    let members = vec![
      window_member("alpha", "src/a.rs", 10, 14),
      window_member("beta", "src/b.rs", 20, 24),
    ];
    let group_fp = Fingerprint::from_bytes(b"group");
    let group = test_support::duplicate_group(DetectionDimension::Line, MatchKind::Exact, group_fp, 1.0, members);
    let result = result_with_line_group(group);

    let mut out = Vec::new();
    cmd_ignore(
      tmp.path(),
      &group_fp.to_hex(),
      Some("intentional family".to_string()),
      &result,
      &mut out,
    )
    .unwrap();

    let file = crate::ignore::load_ignore_file(tmp.path());
    assert_eq!(file.ignore.len(), 1);
    assert_eq!(file.ignore[0].member_fingerprints.len(), 2);
    assert!(file.ignore[0].members[0].contains("src/a.rs:10-14"));
    let text = String::from_utf8(out).unwrap();
    assert!(!text.contains("Note:"), "found groups need no note: {text}");

    // Unmatched fingerprints are still recorded, with a note and
    // without member details.
    let unmatched = Fingerprint::from_bytes(b"unmatched");
    let mut note_out = Vec::new();
    cmd_ignore(tmp.path(), &unmatched.to_hex(), None, &result, &mut note_out).unwrap();
    let file = crate::ignore::load_ignore_file(tmp.path());
    assert_eq!(file.ignore.len(), 2);
    assert!(file.ignore[1].member_fingerprints.is_empty());
    assert!(String::from_utf8(note_out).unwrap().contains("Note:"));
  }

  #[test]
  fn member_locations_parse_registry_style_descriptions() {
    let entry = IgnoreEntry {
      fingerprint:         "ffffffffffffffff".to_string(),
      reason:              None,
      members:             vec![
        "closure body (dupes-treesitter/src/normalizer.rs:422-425)".to_string(),
        "token window dupes-rust/tests/core_with_syn_tests.rs:325-346".to_string(),
      ],
      member_fingerprints: Vec::new(),
    };

    let locations = member_locations(&entry);

    assert!(locations.contains(&("dupes-treesitter/src/normalizer.rs".to_string(), 422, 425)));
    assert!(locations.contains(&("dupes-rust/tests/core_with_syn_tests.rs".to_string(), 325, 346)));
  }

  #[test]
  fn cleanup_dry_run_suggests_successors_for_stale_entries() {
    let tmp = TempDir::new().unwrap();
    let mut ignore_file = crate::ignore::IgnoreFile::default();
    ignore_file.ignore.push(IgnoreEntry {
      fingerprint:         "deadbeefdeadbeef".to_string(),
      reason:              None,
      members:             vec!["line window (src/a.rs:10-14)".to_string()],
      member_fingerprints: Vec::new(),
    });
    crate::ignore::save_ignore_file(tmp.path(), &ignore_file).unwrap();

    let successor_fp = Fingerprint::from_bytes(b"successor");
    let members = vec![
      window_member("shifted", "src/a.rs", 12, 16),
      window_member("shifted", "src/b.rs", 30, 34),
    ];
    let group = test_support::duplicate_group(DetectionDimension::Line, MatchKind::Exact, successor_fp, 1.0, members);
    let result = result_with_line_group(group);

    let mut out = Vec::new();
    cmd_cleanup(tmp.path(), &result, &mut out, true).unwrap();

    let text = String::from_utf8(out).unwrap();
    assert!(text.contains("possible successor"), "{text}");
    assert!(text.contains(&successor_fp.to_hex()), "{text}");
  }
}
