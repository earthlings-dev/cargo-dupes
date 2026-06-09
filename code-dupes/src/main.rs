use std::collections::HashSet;
use std::path::Path;
use std::path::PathBuf;
use std::process;

use clap::{Parser, ValueEnum};
use walkdir::WalkDir;

use dupes_core::analyzer::LanguageAnalyzer;
use dupes_core::cli::{self, CliError, CliOverrides, Command, OutputFormat};
use dupes_core::code_unit::CodeUnit;
use dupes_core::code_unit::DetectionDimension;
use dupes_core::config::AnalysisConfig;
use dupes_python::PythonAnalyzer;
use dupes_rust::RustAnalyzer;

#[derive(Parser)]
#[command(
    name = "code-dupes",
    version,
    about = "Detect duplicate code across multiple languages"
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,

    /// Path to analyze (defaults to current directory).
    #[arg(short, long, global = true)]
    path: Option<PathBuf>,

    /// Language to analyze. Auto-detected from file extensions if omitted.
    #[arg(short, long, global = true)]
    language: Option<Language>,

    /// Minimum AST node count for analysis.
    #[arg(long, global = true)]
    min_nodes: Option<usize>,

    /// Minimum source line count for analysis.
    #[arg(long, global = true)]
    min_lines: Option<usize>,

    /// Similarity threshold (0.0-1.0).
    #[arg(long, global = true)]
    threshold: Option<f64>,

    /// Output format.
    #[arg(long, global = true, default_value = "text")]
    format: OutputFormat,

    /// Exclude patterns (can be repeated).
    #[arg(long, global = true)]
    exclude: Vec<String>,

    /// Exclude test code (language-specific detection).
    #[arg(long, global = true)]
    exclude_tests: bool,

    /// Enable sub-function duplicate detection (if branches, match arms, loop bodies).
    #[arg(long, short = 's', global = true)]
    sub_function: bool,

    /// Disable sub-function duplicate detection.
    #[arg(long, global = true, conflicts_with = "sub_function")]
    no_sub_function: bool,

    /// Minimum AST node count for sub-function units.
    #[arg(long, global = true)]
    min_sub_nodes: Option<usize>,

    /// Disable a detection dimension (can be repeated).
    #[arg(long, global = true)]
    disable_dimension: Vec<DetectionDimension>,

    /// Minimum token count for token-window detection.
    #[arg(long, global = true)]
    token_min_tokens: Option<usize>,

    /// Similarity threshold for normalized token near-duplicates.
    #[arg(long, global = true)]
    token_threshold: Option<f64>,

    /// Minimum line count for line-window detection.
    #[arg(long, global = true)]
    line_min_lines: Option<usize>,
}

#[derive(Clone, ValueEnum)]
enum Language {
    Rust,
    Python,
    Generic,
}

impl Cli {
    /// Resolve the analysis root from CLI input.
    fn root(&self) -> PathBuf {
        self.path
            .clone()
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
    }

    /// Resolve the requested command, defaulting to a full report.
    fn command(&self) -> Command {
        self.command.clone().unwrap_or(Command::Report)
    }

    /// Convert CLI options into shared core overrides.
    fn overrides(&self) -> CliOverrides {
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
            disabled_dimensions: self.disable_dimension.clone(),
            token_min_tokens: self.token_min_tokens,
            token_threshold: self.token_threshold,
            line_min_lines: self.line_min_lines,
            generic_extensions: GENERIC_EXTENSIONS.iter().map(ToString::to_string).collect(),
        }
    }
}

impl Language {
    /// Static file extension registry — avoids constructing analyzers for detection.
    const fn extensions(&self) -> &'static [&'static str] {
        match self {
            Self::Rust => &["rs"],
            Self::Python => &["py", "pyi"],
            Self::Generic => &[],
        }
    }

    const AST: &[Self] = &[Self::Rust, Self::Python];
}

impl std::fmt::Display for Language {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Rust => write!(f, "rust"),
            Self::Python => write!(f, "python"),
            Self::Generic => write!(f, "generic"),
        }
    }
}

/// Extensions scanned by generic token and line detection.
const GENERIC_EXTENSIONS: &[&str] = &[
    "rs", "py", "pyi", "md", "markdown", "toml", "yaml", "yml", "json", "js", "jsx", "ts", "tsx",
    "css", "scss", "sh", "bash", "zsh", "fish", "just", "txt",
];

/// Analyzer used when only generic token/line dimensions are needed.
struct GenericAnalyzer;

impl LanguageAnalyzer for GenericAnalyzer {
    fn file_extensions(&self) -> &[&str] {
        &[]
    }

    fn parse_file(
        &self,
        _path: &Path,
        _source: &str,
        _config: &AnalysisConfig,
    ) -> Result<Vec<CodeUnit>, Box<dyn std::error::Error + Send + Sync>> {
        Ok(Vec::new())
    }
}

/// Create a language analyzer for the given language.
fn resolve_analyzer(language: &Language) -> Box<dyn LanguageAnalyzer> {
    match language {
        Language::Rust => Box::new(RustAnalyzer::new()),
        Language::Python => Box::new(PythonAnalyzer::new()),
        Language::Generic => Box::new(GenericAnalyzer),
    }
}

/// Auto-detect language by scanning for files matching known extensions.
///
/// Performs a single directory walk (instead of one per language) and collects
/// file extensions, then matches against `Language::ALL`. Returns an error if
/// multiple languages are detected — the user must specify `--language` to
/// disambiguate.
fn auto_detect_language(root: &std::path::Path) -> Result<Language, CliError> {
    // Collect all file extensions found in a single walk.
    let mut found_extensions = HashSet::new();

    for entry in WalkDir::new(root)
        .into_iter()
        .filter_entry(|e| {
            let path = e.path();
            if path.is_dir()
                && let Some(name) = path.file_name().and_then(|n| n.to_str())
            {
                if name == "target" {
                    return false;
                }
                if name.starts_with('.') && path != root {
                    return false;
                }
            }
            true
        })
        .flatten()
    {
        let path = entry.path();
        if path.is_file()
            && let Some(ext) = path.extension().and_then(|e| e.to_str())
        {
            found_extensions.insert(ext.to_ascii_lowercase());
        }
    }

    // Match found extensions against known languages.
    let detected: Vec<Language> = Language::AST
        .iter()
        .filter(|lang| {
            lang.extensions()
                .iter()
                .any(|ext| found_extensions.contains(*ext))
        })
        .cloned()
        .collect();

    match detected.len() {
        0 => {
            if GENERIC_EXTENSIONS
                .iter()
                .any(|ext| found_extensions.contains(*ext))
            {
                Ok(Language::Generic)
            } else {
                Err(CliError::NoRecognizedFiles)
            }
        }
        1 => Ok(detected.into_iter().next().unwrap()),
        _ => Err(CliError::AmbiguousLanguage(
            detected.iter().map(ToString::to_string).collect(),
        )),
    }
}

fn main() {
    let cli = Cli::parse();
    let root = cli.root();
    let command = cli.command();
    let stdout = std::io::stdout();
    let mut writer = stdout.lock();

    if let Err(e) = run_command(&cli, &root, &command, &mut writer) {
        exit_with_error(&e);
    }
}

/// Run the requested command.
fn run_command(
    cli: &Cli,
    root: &Path,
    command: &Command,
    writer: &mut impl std::io::Write,
) -> cli::CliResult {
    match command {
        Command::Ignore {
            fingerprint,
            reason,
        } => cli::cmd_ignore(root, fingerprint, reason.clone(), writer),
        Command::Ignored => cli::cmd_ignored(root, writer),
        _ => run_analysis_command(cli, root, command, writer),
    }
}

/// Run a command that needs duplicate analysis first.
fn run_analysis_command(
    cli_args: &Cli,
    root: &Path,
    command: &Command,
    writer: &mut impl std::io::Write,
) -> cli::CliResult {
    let language = cli_args
        .language
        .clone()
        .map_or_else(|| auto_detect_language(root), Ok)?;
    let analyzer = resolve_analyzer(&language);
    let overrides = cli_args.overrides();
    let output = cli::run_analysis(analyzer.as_ref(), root, cli_args.format, &overrides)?;

    for warning in &output.result.warnings {
        eprintln!("Warning: {warning}");
    }

    dispatch_analysis_command(root, command, &output, writer)
}

/// Dispatch a command after analysis has completed.
fn dispatch_analysis_command(
    root: &Path,
    command: &Command,
    output: &cli::AnalysisOutput,
    writer: &mut impl std::io::Write,
) -> cli::CliResult {
    let reporter: &dyn dupes_core::output::Reporter = &*output.reporter;

    match command {
        Command::Stats => cli::cmd_stats(&output.result, reporter, writer),
        Command::Report => cli::cmd_report(&output.result, reporter, writer),
        Command::Check {
            max_exact,
            max_near,
            max_exact_percent,
            max_near_percent,
        } => cli::cmd_check(
            &output.config,
            &output.result,
            reporter,
            writer,
            &cli::CheckThresholds {
                max_exact: *max_exact,
                max_near: *max_near,
                max_exact_percent: *max_exact_percent,
                max_near_percent: *max_near_percent,
            },
        ),
        Command::Cleanup { dry_run } => cli::cmd_cleanup(root, &output.result, writer, *dry_run),
        Command::Ignore { .. } | Command::Ignored => Ok(()),
    }
}

/// Exit with the CLI's documented exit code.
fn exit_with_error(error: &CliError) -> ! {
    if !matches!(error, CliError::CheckFailed) {
        eprintln!("Error: {error}");
    }
    process::exit(error.exit_code());
}
